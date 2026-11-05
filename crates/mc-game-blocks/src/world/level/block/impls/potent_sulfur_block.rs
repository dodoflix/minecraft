//! `net.minecraft.world.level.block.PotentSulfurBlock` (extends `BaseEntityBlock`); the geyser/nausea tickers live in
//! `PotentSulfurBlockEntity`.
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;
use crate::world::block_entity::PotentSulfurBlockEntity;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_data::enums::PotentSulfurState;
use mc_data::fluid::Fluid;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

pub struct PotentSulfurBlock;

fn is_source_if_fluid(below_state: BlockState) -> bool {
    let fluid = below_state.get_fluid_state();
    fluid.is_empty() || fluid.is_source()
}

/// `PotentSulfurBlock.validBlockState`.
fn valid_block_state(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) -> BlockState {
    if !level.get_fluid_state(pos.above()).is_source_of_type(Fluid::WATER) {
        return state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Dry);
    }
    let below_state = level.get_block_state(pos.below());
    if below_state.is_tag("causes_continuous_geyser_eruptions") && is_source_if_fluid(below_state) {
        state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Continuous)
    } else if below_state.is_tag("causes_periodic_geyser_eruptions") && is_source_if_fluid(below_state) {
        let is_geyser = matches!(state.get(props::POTENT_SULFUR_STATE), PotentSulfurState::Erupting | PotentSulfurState::Dormant);
        if !is_geyser
            && let Some(entity) = level.get_block_entity_mut(pos).and_then(|be| be.as_any_mut().downcast_mut::<PotentSulfurBlockEntity>())
        {
            entity.reset_countdown();
        }
        if state.get(props::POTENT_SULFUR_STATE) == PotentSulfurState::Erupting { state } else { state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Dormant) }
    } else {
        state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Wet)
    }
}

impl BlockBehaviour for PotentSulfurBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        valid_block_state(state, level, pos)
    });
    fn get_state_for_placement(&self, block: Block, level: &mut mc_game_core::server::level::ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(valid_block_state(block.default_state(), level, context.get_clicked_pos()))
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        let geyser_state = state.get(props::POTENT_SULFUR_STATE);
        if geyser_state == PotentSulfurState::Erupting || geyser_state == PotentSulfurState::Continuous {
            level.block_event(pos, state.block(), 0, 0);
            let sound = if geyser_state == PotentSulfurState::Continuous { SoundEvent::BLOCK_POTENT_SULFUR_GEYSER_CONTINUOUS_ERUPTION } else { SoundEvent::BLOCK_POTENT_SULFUR_GEYSER_ERUPTION };
            level.play_sound(None, pos, sound, SoundSource::Blocks, 1.0, 1.0);
            level.game_event_at_block(GameEvent::BLOCK_ACTIVATE, pos, &GameEventContext::of_state(Some(state)));
        }
    }
    fn trigger_event(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, _b0: i32, _b1: i32) -> bool {
        let game_time = level.get_game_time();
        if let Some(entity) = level.get_block_entity_mut(pos).and_then(|be| be.as_any_mut().downcast_mut::<PotentSulfurBlockEntity>()) {
            entity.eruption_tick = game_time;
        }
        true
    }
}

register_block_class!("PotentSulfurBlock", |_| Box::new(PotentSulfurBlock));
