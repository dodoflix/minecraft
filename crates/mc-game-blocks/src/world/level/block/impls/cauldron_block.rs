//! `net.minecraft.world.level.block.CauldronBlock` (the empty cauldron).
use super::abstract_cauldron_block;
use super::cauldron_interactions::CauldronKind;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::item::{InteractionHand, InteractionResult, ItemStack};
use crate::world::level::gameevent::GameEventContext;
use crate::world::level::precipitation::Precipitation;
use mc_data::GameEvent;

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
    fn use_item_on(&self, stack: &ItemStack, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, hand: InteractionHand, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        abstract_cauldron_block::use_item_on(CauldronKind::Empty, stack, state, level, pos, player, hand)
    }
    fn get_analog_output_signal(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _direction: Direction) -> i32 {
        0
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        false
    }
    fn handle_precipitation(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, precipitation: Precipitation) {
        if should_handle_precipitation(level, precipitation) {
            if precipitation == Precipitation::Rain {
                level.set_block_and_update(pos, Block::WATER_CAULDRON.default_state());
                level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::default());
            } else if precipitation == Precipitation::Snow {
                level.set_block_and_update(pos, Block::POWDER_SNOW_CAULDRON.default_state());
                level.game_event_at_block(GameEvent::BLOCK_CHANGE, pos, &GameEventContext::default());
            }
        }
    }
}

register_block_class!("CauldronBlock", |_| Box::new(CauldronBlock));
