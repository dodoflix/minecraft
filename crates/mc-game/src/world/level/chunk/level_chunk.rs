//! Game-layer `LevelChunk`: the `mc_world` chunk data plus the live block entities, their tickers and the game event
//! listener registries. The behaviour that needs the level (`setBlockState`, block entity creation/removal, tickers,
//! post-processing) is `impl ServerLevel` in `server/level/level_chunk_ops.rs` (the chunk lives inside the level's chunk
//! map, so those operations are level methods addressed by position).
use crate::server::level::FullChunkStatus;
use crate::world::block_entity::BlockEntity;
use mc_core::pos::BlockPos;
use mc_world::chunk::ProtoChunk;
use mc_world::palette::PalettedContainerFactory;
use crate::world::level::gameevent::EuclideanGameEventListenerRegistry;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// One entry of the level's block entity ticker list (vanilla `RebindableTickingBlockEntityWrapper`): the position of
/// the block entity it currently ticks, or `None` once it was rebound to `NULL_TICKER` (removed).
#[derive(Debug, Default)]
pub struct TickerCell(pub Cell<Option<BlockPos>>);

pub struct LevelChunk {
    pub data: mc_world::chunk::LevelChunk,
    pub block_entities: HashMap<BlockPos, Box<dyn BlockEntity>>,
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
        Self { data: mc_world::chunk::LevelChunk::from_proto(proto, factory), block_entities: HashMap::new(), full_status: FullChunkStatus::Inaccessible, tickers_in_level: HashMap::new(), game_event_registries: HashMap::new() }
    }

    pub fn from_data(data: mc_world::chunk::LevelChunk) -> Self {
        Self { data, block_entities: HashMap::new(), full_status: FullChunkStatus::Inaccessible, tickers_in_level: HashMap::new(), game_event_registries: HashMap::new() }
    }

    pub fn pos(&self) -> mc_core::pos::ChunkPos {
        self.data.pos()
    }
}

impl LevelChunk {
    /// The block entity NBT of `copyOf` (`getBlockEntityNbtForSaving` per position): a live block entity is saved with
    /// `saveWithFullMetadata` + `keepPacked: false`; pending raw tags (types without an implementation yet) are kept
    /// as they are so nothing is lost.
    pub fn block_entity_nbt_for_saving(&self) -> Vec<mc_nbt::CompoundTag> {
        let mut out: Vec<mc_nbt::CompoundTag> = Vec::new();
        for be in self.block_entities.values() {
            if be.is_removed() {
                continue;
            }
            let mut tag = be.save_with_full_metadata();
            tag.put_boolean("keepPacked", false);
            out.push(tag);
        }
        for (pos, tag) in &self.data.access.block_entities {
            if !self.block_entities.contains_key(pos) {
                out.push(tag.clone());
            }
        }
        out
    }
}

impl LevelChunk {
    /// `registerAllBlockEntitiesAfterLevelLoad` / `promotePendingBlockEntity`: instantiate the pending block entity NBT
    /// of types that have an implementation (`BlockEntityType.create` + `loadAdditional`). Tags of types without an
    /// implementation (or whose block state does not match) stay raw in `data.access.block_entities`, so nothing is
    /// lost when the chunk is saved.
    pub fn promote_pending_block_entities(&mut self) {
        let positions: Vec<BlockPos> = self.data.access.block_entities.keys().copied().collect();
        for pos in positions {
            let Some(tag) = self.data.access.block_entities.get(&pos) else { continue };
            let Some(ty) = tag.get_string("id").and_then(mc_data::BlockEntityType::by_name) else { continue };
            let state = self.data.get_block_state(pos);
            if !ty.data().valid_blocks.contains(&state.block()) {
                continue;
            }
            if let Some(mut be) = crate::world::block_entity::create_block_entity(ty, pos, state) {
                be.load_additional(tag);
                self.data.access.block_entities.remove(&pos);
                self.block_entities.insert(pos, be);
            }
        }
    }
}
