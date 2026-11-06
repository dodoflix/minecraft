//! `EnderChestBlockEntity`: only the lid state and the opener counter; the contents live in the player's
//! `PlayerEnderChestContainer` (`isActiveChest` tells which chest the player has open).

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{BlockEntityType, SoundEvent};
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::container_openers::{
    ChestLidController, ContainerOpenersCounter, OpenersHost, player_has_ender_chest_active_at,
};
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, ContainerUser,
};
use mc_game_core::world::entity::Entity;
use mc_game_core::world::level::LevelAccessor;
use mc_protocol::packets::play::level::SoundSource;

pub struct EnderChestBlockEntity {
    base: BlockEntityData,
    lid: ChestLidController,
    openers: ContainerOpenersCounter,
}

struct EnderChestHost;

impl EnderChestHost {
    fn play(level: &mut ServerLevel, pos: BlockPos, sound: SoundEvent) {
        let pitch = LevelAccessor::random(level).next_float() * 0.1 + 0.9;
        level.play_sound_at(
            None,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
            sound,
            SoundSource::Blocks,
            0.5,
            pitch,
        );
    }
}

impl OpenersHost for EnderChestHost {
    fn on_open(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState) {
        Self::play(level, pos, SoundEvent::BLOCK_ENDER_CHEST_OPEN);
    }
    fn on_close(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState) {
        Self::play(level, pos, SoundEvent::BLOCK_ENDER_CHEST_CLOSE);
    }
    fn opener_count_changed(
        &self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
        _previous: i32,
        current: i32,
    ) {
        level.block_event(pos, Block::ENDER_CHEST, 1, current);
    }
    fn is_own_container(&self, player: &dyn Entity, pos: BlockPos) -> bool {
        player_has_ender_chest_active_at(player, pos)
    }
}

impl EnderChestBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::ENDER_CHEST, pos, state),
            lid: ChestLidController::default(),
            openers: ContainerOpenersCounter::default(),
        }
    }

    pub fn recheck_open(&mut self, level: &mut ServerLevel) {
        if !self.base.removed {
            let (pos, state) = (self.base.pos, self.base.block_state);
            self.openers
                .recheck_openers(&EnderChestHost, level, pos, state);
        }
    }

    pub fn get_open_ness(&self, a: f32) -> f32 {
        self.lid.get_openness(a)
    }
}

impl BlockEntity for EnderChestBlockEntity {
    block_entity_base!();

    fn trigger_event(&mut self, _level: &mut ServerLevel, b0: i32, b1: i32) -> bool {
        if b0 == 1 {
            self.lid.should_be_open(b1 > 0);
            true
        } else {
            false
        }
    }

    fn start_open(&mut self, level: &mut ServerLevel, user: &ContainerUser) {
        if !self.base.removed && !user.spectator {
            let (pos, state) = (self.base.pos, self.base.block_state);
            self.openers.increment_openers(
                &EnderChestHost,
                user.id,
                level,
                pos,
                state,
                user.interaction_range,
            );
        }
    }

    fn stop_open(&mut self, level: &mut ServerLevel, user: &ContainerUser) {
        if !self.base.removed && !user.spectator {
            let (pos, state) = (self.base.pos, self.base.block_state);
            self.openers
                .decrement_openers(&EnderChestHost, user.id, level, pos, state);
        }
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static ENDER_CHEST: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::ENDER_CHEST,
    create: |pos, state| Box::new(EnderChestBlockEntity::new(pos, state)),
};
