//! Game-layer `LevelChunk`: the `mc_world` chunk data plus the live block entities, their tickers and the game event
//! listener registries. The behaviour that needs the level (`setBlockState`, block entity creation/removal, tickers,
//! post-processing) is `impl ServerLevel` in `server/level/level_chunk_ops.rs` (the chunk lives inside the level's chunk
//! map, so those operations are level methods addressed by position).
use crate::server::level::FullChunkStatus;
use crate::world::block_entity::BlockEntity;
use crate::world::level::gameevent::EuclideanGameEventListenerRegistry;
use mc_core::java_collections::{FastOpenHashMap, block_pos_hash};
use mc_core::pos::BlockPos;
use mc_nbt::CompoundTag;
use mc_world::chunk::{ProtoChunk, block_entity_pos_from_tag};
use mc_world::palette::PalettedContainerFactory;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// One entry of the level's block entity ticker list (vanilla `RebindableTickingBlockEntityWrapper`): the position of
/// the block entity it currently ticks, or `None` once it was rebound to `NULL_TICKER` (removed).
#[derive(Debug, Default)]
pub struct TickerCell(pub Cell<Option<BlockPos>>);

/// `LevelChunk.blockEntities`, an `Object2ObjectOpenHashMap` keyed by position: iteration (chunk packet, save,
/// `registerAllBlockEntitiesAfterLevelLoad`) is in vanilla's order. A block entity that is taken out while its code
/// runs (`take`) leaves its key in the table, so it keeps its place when it is put back; to every accessor the key is
/// absent meanwhile.
pub struct BlockEntityMap(FastOpenHashMap<BlockPos, Option<Box<dyn BlockEntity>>>);

impl BlockEntityMap {
    /// `new Object2ObjectOpenHashMap<>()`.
    pub fn new() -> Self {
        Self(FastOpenHashMap::new(16, block_pos_hash))
    }

    pub fn get(&self, pos: &BlockPos) -> Option<&dyn BlockEntity> {
        self.0.get(pos)?.as_deref()
    }

    pub fn get_mut(&mut self, pos: &BlockPos) -> Option<&mut Box<dyn BlockEntity>> {
        self.0.get_mut(pos)?.as_mut()
    }

    pub fn contains_key(&self, pos: &BlockPos) -> bool {
        self.get(pos).is_some()
    }

    /// `put`: the previous block entity of `pos`, if any.
    pub fn insert(
        &mut self,
        pos: BlockPos,
        block_entity: Box<dyn BlockEntity>,
    ) -> Option<Box<dyn BlockEntity>> {
        self.0.put(pos, Some(block_entity)).flatten()
    }

    /// `remove`: the key goes away (a taken block entity's key too).
    pub fn remove(&mut self, pos: &BlockPos) -> Option<Box<dyn BlockEntity>> {
        self.0.remove(pos).flatten()
    }

    /// The block entity out of its slot for the duration of its own code; pair with [`Self::put_back`].
    pub fn take(&mut self, pos: &BlockPos) -> Option<Box<dyn BlockEntity>> {
        self.0.get_mut(pos)?.take()
    }

    /// Returns a taken block entity to its slot unless the position got a block entity (or lost its key) meanwhile:
    /// a lost key is inserted again.
    pub fn put_back(&mut self, pos: BlockPos, block_entity: Box<dyn BlockEntity>) {
        match self.0.get_mut(&pos) {
            Some(slot @ None) => *slot = Some(block_entity),
            Some(Some(_)) => {}
            None => {
                self.0.put(pos, Some(block_entity));
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&BlockPos, &dyn BlockEntity)> {
        self.0
            .iter()
            .filter_map(|(p, be)| Some((p, be.as_deref()?)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&BlockPos, &mut Box<dyn BlockEntity>)> {
        self.0
            .iter_mut()
            .filter_map(|(p, be)| Some((p, be.as_mut()?)))
    }

    pub fn keys(&self) -> impl Iterator<Item = &BlockPos> {
        self.iter().map(|e| e.0)
    }

    pub fn values(&self) -> impl Iterator<Item = &dyn BlockEntity> {
        self.iter().map(|e| e.1)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn BlockEntity>> {
        self.iter_mut().map(|e| e.1)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

impl<'a> IntoIterator for &'a BlockEntityMap {
    type Item = (&'a BlockPos, &'a dyn BlockEntity);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl Default for BlockEntityMap {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LevelChunk {
    pub data: mc_world::chunk::LevelChunk,
    pub block_entities: BlockEntityMap,
    /// `postLoad` of a chunk read from disk: its block entity tags, in list order, until `run_post_load`.
    pub post_load_block_entities: Vec<CompoundTag>,
    pub full_status: FullChunkStatus,
    /// `tickersInLevel`.
    pub tickers_in_level: HashMap<BlockPos, Rc<TickerCell>>,
    /// `gameEventListenerRegistrySections` keyed by section y.
    pub game_event_registries: HashMap<i32, EuclideanGameEventListenerRegistry>,
}

impl LevelChunk {
    /// `new LevelChunk(level, protoChunk, postLoad)`: the data part (block entities stay pending raw NBT in
    /// `data.access.block_entities` until registered).
    pub fn from_proto(proto: ProtoChunk, factory: &PalettedContainerFactory) -> Self {
        Self::from_data(
            mc_world::chunk::LevelChunk::from_proto(proto, factory),
            Vec::new(),
        )
    }

    /// A chunk read from disk with the block entity tags its `postLoad` instantiates.
    pub fn from_data(
        data: mc_world::chunk::LevelChunk,
        post_load_block_entities: Vec<CompoundTag>,
    ) -> Self {
        Self {
            data,
            block_entities: BlockEntityMap::new(),
            post_load_block_entities,
            full_status: FullChunkStatus::Inaccessible,
            tickers_in_level: HashMap::new(),
            game_event_registries: HashMap::new(),
        }
    }

    pub fn pos(&self) -> mc_core::pos::ChunkPos {
        self.data.pos()
    }
}

impl LevelChunk {
    /// The block entity NBT of `copyOf` (`getBlockEntityNbtForSaving` per position of `getBlockEntitiesPos`): a live
    /// block entity is saved with `saveWithFullMetadata` + `keepPacked: false`; pending raw tags (types without an
    /// implementation yet) are kept as they are so nothing is lost.
    pub fn block_entity_nbt_for_saving(&self) -> Vec<CompoundTag> {
        // Before `runPostLoad` nothing has moved yet (vanilla's `ImposterProtoChunk` is not saved until then).
        if !self.post_load_block_entities.is_empty() {
            return self.post_load_block_entities.clone();
        }
        let access = &self.data.access;
        let mut out = Vec::new();
        for pos in access.get_block_entities_pos(self.block_entities.keys().copied()) {
            match self.block_entities.get(&pos).filter(|be| !be.is_removed()) {
                Some(be) => {
                    let mut tag = be.save_with_full_metadata();
                    tag.put_boolean("keepPacked", false);
                    out.push(tag);
                }
                None => out.extend(access.block_entities.get(&pos).cloned()),
            }
        }
        out
    }
}

impl LevelChunk {
    /// The live block entity of `tag` (`BlockEntityType.create` + `loadAdditional`), `None` for a type without an
    /// implementation or whose block state does not match.
    fn instantiate(&self, pos: BlockPos, tag: &CompoundTag) -> Option<Box<dyn BlockEntity>> {
        let ty = tag
            .get_string("id")
            .and_then(mc_data::BlockEntityType::by_name)?;
        let state = self.data.get_block_state(pos);
        if !ty.data().valid_blocks.contains(&state.block()) {
            return None;
        }
        let mut be = crate::world::block_entity::create_block_entity(ty, pos, state)?;
        be.load_additional(tag);
        Some(be)
    }

    /// `runPostLoad` for the block entities: a tag with `keepPacked` becomes pending NBT, any other one a live block
    /// entity (`loadStatic` + `setBlockEntity`), in list order. Tags that cannot be instantiated stay raw pending NBT,
    /// so nothing is lost when the chunk is saved.
    pub fn run_post_load(&mut self) {
        for tag in std::mem::take(&mut self.post_load_block_entities) {
            let pos = block_entity_pos_from_tag(self.pos(), &tag);
            let live = if tag.get_boolean_or("keepPacked", false) {
                None
            } else {
                self.instantiate(pos, &tag)
            };
            match live {
                Some(be) => {
                    self.block_entities.insert(pos, be);
                }
                None => self.data.access.set_block_entity_nbt(tag),
            }
        }
    }

    /// The pending block entity NBT of a chunk promoted from a proto chunk becomes live block entities, in the pending
    /// map's order (vanilla promotes them on first access, at the latest in `postProcessGeneration`). Tags that cannot
    /// be instantiated stay raw in `data.access.block_entities`.
    pub fn promote_pending_block_entities(&mut self) {
        let positions: Vec<BlockPos> = self.data.access.block_entities.keys().copied().collect();
        for pos in positions {
            let Some(tag) = self.data.access.block_entities.get(&pos) else {
                continue;
            };
            if let Some(be) = self.instantiate(pos, tag) {
                self.data.access.block_entities.remove(&pos);
                self.block_entities.insert(pos, be);
            }
        }
    }
}
