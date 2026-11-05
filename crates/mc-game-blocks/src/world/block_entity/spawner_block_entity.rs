//! `SpawnerBlockEntity`: the monster spawner. All logic is `BaseSpawner`'s.

use mc_game_core::block_entity_base;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, set_changed};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::BlockGetter;
use mc_game_core::world::level::base_spawner::{BaseSpawner, BaseSpawnerHost};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{BlockEntityType, EntityType};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

pub use mc_game_core::world::block_entity::Spawner;

/// The anonymous `BaseSpawner` subclass of `SpawnerBlockEntity`.
struct SpawnerHost;

impl BaseSpawnerHost for SpawnerHost {
    fn broadcast_event(&self, level: &mut ServerLevel, pos: BlockPos, id: i32) {
        level.block_event(pos, Block::SPAWNER, id, 0);
    }

    fn next_spawn_data_changed(&self, level: &mut ServerLevel, pos: BlockPos) {
        let state = level.get_block_state(pos);
        level.send_block_updated(pos, state, state, 260);
    }
}

pub struct SpawnerBlockEntity {
    base: BlockEntityData,
    pub spawner: BaseSpawner,
}

impl SpawnerBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::MOB_SPAWNER, pos, state), spawner: BaseSpawner::new(Box::new(SpawnerHost)) }
    }

    fn changed(&self, level: &mut ServerLevel) {
        set_changed(level, self.get_block_pos(), self.get_block_state());
    }
}

impl Spawner for SpawnerBlockEntity {
    fn set_entity_id(&mut self, level: &mut ServerLevel, ty: EntityType) {
        let pos = self.get_block_pos();
        self.spawner.set_entity_id(ty, level, pos);
        self.changed(level);
    }

    fn set_entity_data(&mut self, level: &mut ServerLevel, ty: EntityType, data: &CompoundTag) {
        let pos = self.get_block_pos();
        self.spawner.set_entity_data(ty, data, level, pos);
        self.changed(level);
    }
}

impl BlockEntity for SpawnerBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.spawner.load(tag);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.spawner.save(tag);
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        self.spawner.server_tick(level, pos);
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }

    /// `saveCustomOnly` without `SpawnPotentials`.
    fn get_update_tag(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_additional(&mut tag);
        tag.remove("SpawnPotentials");
        tag
    }

    fn trigger_event(&mut self, _level: &mut ServerLevel, b0: i32, _b1: i32) -> bool {
        self.spawner.on_event_triggered(b0)
    }

    fn as_spawner_mut(&mut self) -> Option<&mut dyn Spawner> {
        Some(self)
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static MOB_SPAWNER: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::MOB_SPAWNER, create: |pos, state| Box::new(SpawnerBlockEntity::new(pos, state)) };
