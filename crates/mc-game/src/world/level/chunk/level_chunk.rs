//! Game-layer `LevelChunk`: the `mc_world` chunk data plus the live block entities. (Placeholder shape agreed with the
//! LevelTick slice, which replaces this file with the full vanilla `LevelChunk` behaviour; the three public fields and
//! `from_proto` are the contract.)
use crate::server::level::FullChunkStatus;
use crate::world::block_entity::BlockEntity;
use mc_core::pos::BlockPos;
use mc_world::chunk::ProtoChunk;
use mc_world::palette::PalettedContainerFactory;
use std::collections::HashMap;

pub struct LevelChunk {
    pub data: mc_world::chunk::LevelChunk,
    pub block_entities: HashMap<BlockPos, Box<dyn BlockEntity>>,
    pub full_status: FullChunkStatus,
}

impl LevelChunk {
    /// `new LevelChunk(level, protoChunk, postLoad)`: the data part (block entities stay pending raw NBT in
    /// `data.access.block_entities` until registered).
    pub fn from_proto(proto: ProtoChunk, factory: &PalettedContainerFactory) -> Self {
        Self { data: mc_world::chunk::LevelChunk::from_proto(proto, factory), block_entities: HashMap::new(), full_status: FullChunkStatus::Inaccessible }
    }

    pub fn from_data(data: mc_world::chunk::LevelChunk) -> Self {
        Self { data, block_entities: HashMap::new(), full_status: FullChunkStatus::Inaccessible }
    }

    pub fn pos(&self) -> mc_core::pos::ChunkPos {
        self.data.pos()
    }
}

impl LevelChunk {
    /// The block entity NBT of `copyOf`: live block entities saved with `id`/`x`/`y`/`z` (`saveWithFullMetadata`) plus the
    /// pending raw tags of types that have no implementation yet (kept so nothing is lost).
    pub fn block_entity_nbt_for_saving(&self) -> Vec<mc_nbt::CompoundTag> {
        let mut out: Vec<mc_nbt::CompoundTag> = Vec::new();
        for (pos, be) in &self.block_entities {
            if be.is_removed() {
                continue;
            }
            let mut tag = mc_nbt::CompoundTag::new();
            be.save_additional(&mut tag);
            tag.put_string("id", be.get_type().name());
            tag.put_int("x", pos.x());
            tag.put_int("y", pos.y());
            tag.put_int("z", pos.z());
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
