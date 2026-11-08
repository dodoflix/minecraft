//! `net.minecraft.world.level.block.CauldronBlock` (the empty cauldron).
use super::abstract_cauldron_block;
use super::cauldron_interactions::CauldronKind;
use mc_core::phys::shapes::VoxelShape;
use mc_data::GameEvent;
use mc_data::fluid::Fluid;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::item::{InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::precipitation::Precipitation;

pub struct CauldronBlock;

/// `CauldronBlock.shouldHandlePrecipitation(level, precipitation)`.
pub fn should_handle_precipitation(level: &mut ServerLevel, precipitation: Precipitation) -> bool {
    match precipitation {
        Precipitation::Rain => LevelAccessor::random(level).next_float() < 0.05,
        Precipitation::Snow => LevelAccessor::random(level).next_float() < 0.1,
        _ => false,
    }
}

impl BlockBehaviour for CauldronBlock {
    fn get_interaction_shape(
        &self,
        _state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
    ) -> VoxelShape {
        abstract_cauldron_block::interaction_shape()
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        abstract_cauldron_block::tick(self, state, level, pos);
    }
    fn can_receive_stalactite_drip(&self, _fluid: Fluid) -> bool {
        true
    }
    fn receive_stalactite_drip(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        fluid: Fluid,
    ) {
        let (new_state, event) = if fluid == Fluid::WATER {
            (Block::WATER_CAULDRON.default_state(), 1047)
        } else if fluid == Fluid::LAVA {
            (Block::LAVA_CAULDRON.default_state(), 1046)
        } else {
            return;
        };
        level.set_block_and_update(pos, new_state);
        level.game_event_at_block(
            GameEvent::BLOCK_CHANGE,
            pos,
            &GameEventContext::of_state(Some(new_state)),
        );
        level.level_event(None, event, pos, 0);
    }
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        abstract_cauldron_block::use_item_on(
            CauldronKind::Empty,
            stack,
            state,
            level,
            pos,
            player,
            hand,
        )
    }
    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        0
    }
    fn is_pathfindable(
        &self,
        _state: BlockState,
        _ty: mc_data::block::PathComputationType,
    ) -> bool {
        false
    }
    fn handle_precipitation(
        &self,
        _state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        precipitation: Precipitation,
    ) {
        if should_handle_precipitation(level, precipitation) {
            if precipitation == Precipitation::Rain {
                level.set_block_and_update(pos, Block::WATER_CAULDRON.default_state());
                level.game_event_at_block(
                    GameEvent::BLOCK_CHANGE,
                    pos,
                    &GameEventContext::default(),
                );
            } else if precipitation == Precipitation::Snow {
                level.set_block_and_update(pos, Block::POWDER_SNOW_CAULDRON.default_state());
                level.game_event_at_block(
                    GameEvent::BLOCK_CHANGE,
                    pos,
                    &GameEventContext::default(),
                );
            }
        }
    }
}

register_block_class!("CauldronBlock", |_| Box::new(CauldronBlock));
