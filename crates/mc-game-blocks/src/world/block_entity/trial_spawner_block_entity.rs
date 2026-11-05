//! `TrialSpawnerBlockEntity`: owns a [`TrialSpawner`] and mirrors its block state into it.

use mc_game_core::block_entity_base;
use super::spawner_block_entity::Spawner;
use super::trialspawner::TrialSpawner;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, set_changed};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::BlockGetter;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::TrialSpawnerState;
use mc_data::{BlockEntityType, EntityType, props};
use mc_nbt::CompoundTag;
use mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket;

pub struct TrialSpawnerBlockEntity {
    base: BlockEntityData,
    pub trial_spawner: TrialSpawner,
}

impl TrialSpawnerBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::TRIAL_SPAWNER, pos, state), trial_spawner: TrialSpawner::new(pos, state) }
    }

    fn changed(&self, level: &mut ServerLevel) {
        set_changed(level, self.get_block_pos(), self.get_block_state());
    }
}

impl Spawner for TrialSpawnerBlockEntity {
    fn set_entity_id(&mut self, level: &mut ServerLevel, ty: EntityType) {
        self.trial_spawner.override_entity_to_spawn(level, ty);
        self.changed(level);
    }

    fn set_entity_data(&mut self, level: &mut ServerLevel, ty: EntityType, data: &CompoundTag) {
        self.trial_spawner.override_entity_data(level, ty, data);
        self.changed(level);
    }
}

impl BlockEntity for TrialSpawnerBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.trial_spawner.load(tag);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.trial_spawner.store(tag);
    }

    fn set_block_state(&mut self, state: BlockState) {
        self.base.block_state = state;
        self.trial_spawner.set_cached_block_state(state);
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        let pos = self.get_block_pos();
        let state = level.get_block_state(pos);
        let ominous = state.has_property(props::OMINOUS) && state.get(props::OMINOUS);
        self.trial_spawner.tick_server(level, pos, ominous);
    }

    fn get_update_packet(&self) -> Option<ClientboundBlockEntityDataPacket> {
        Some(ClientboundBlockEntityDataPacket { pos: self.get_block_pos(), r#type: self.get_type(), tag: self.get_update_tag() })
    }

    fn get_update_tag(&self) -> CompoundTag {
        let state = self.get_block_state();
        self.trial_spawner.data.get_update_tag(state.has_property(props::TRIAL_SPAWNER_STATE) && state.get(props::TRIAL_SPAWNER_STATE) == TrialSpawnerState::Active)
    }

    fn as_spawner_mut(&mut self) -> Option<&mut dyn Spawner> {
        Some(self)
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static TRIAL_SPAWNER: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::TRIAL_SPAWNER, create: |pos, state| Box::new(TrialSpawnerBlockEntity::new(pos, state)) };
