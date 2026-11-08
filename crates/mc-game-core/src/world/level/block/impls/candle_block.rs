//! `net.minecraft.world.level.block.CandleBlock` (extends `AbstractCandleBlock`, implements `SimpleWaterloggedBlock`).
use super::abstract_candle_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::ServerPlayer;
use crate::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack};
use crate::world::level::explosion::Explosion;
use mc_data::fluid::{Fluid, FluidState};

pub struct CandleBlock;

/// `CandleBlock.canLight(state)`.
pub fn can_light(state: BlockState) -> bool {
    state.is_tag("candles")
        && state.has_property(props::LIT)
        && state.has_property(props::WATERLOGGED)
        && !state.get(props::LIT)
        && !state.get(props::WATERLOGGED)
}

impl BlockBehaviour for CandleBlock {
    fn use_item_on(
        &self,
        stack: &ItemStack,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hand: InteractionHand,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if stack.is_empty() && player.player.abilities.may_build && state.get(props::LIT) {
            abstract_candle_block::extinguish(Some(player.base_id()), state, level, pos);
            InteractionResult::SUCCESS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        let held = context.use_on.get_item_in_hand();
        if !context.is_secondary_use_active()
            && Some(held.item()) == state.block().item()
            && !held.is_empty()
            && state.get(props::CANDLES) < 4
        {
            true
        } else {
            state.can_be_replaced()
                && (held.is_empty() || Some(held.item()) != state.block().item())
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        let state = level.get_block_state(context.get_clicked_pos());
        if state.is_block(block) {
            Some(state.cycle(props::CANDLES))
        } else {
            let is_water_source =
                level.get_fluid_state(context.get_clicked_pos()).get_type() == Fluid::WATER;
            Some(
                block
                    .default_state()
                    .set(props::WATERLOGGED, is_water_source),
            )
        }
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    );
    crate::simple_waterlogged!(no_place_liquid);
    fn place_liquid(
        &self,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        state: BlockState,
        fluid_state: FluidState,
    ) -> bool {
        if !state.get(props::WATERLOGGED) && fluid_state.get_type() == Fluid::WATER {
            let new_state = state.set(props::WATERLOGGED, true);
            if state.get(props::LIT) {
                abstract_candle_block::extinguish(None, new_state, level, pos);
            } else {
                level.set_block_and_update(pos, new_state);
            }
            let delay = crate::world::level::material::fluid_behaviour(fluid_state.get_type())
                .get_tick_delay(&*level);
            level.schedule_fluid_tick(
                pos,
                fluid_state.get_type(),
                delay,
                mc_world::ticks::TickPriority::Normal,
            );
            true
        } else {
            false
        }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        helpers::can_support_center(level, pos.below(), Direction::Up)
    }
    fn on_projectile_hit(
        &self,
        level: &mut ServerLevel,
        state: BlockState,
        hit: &mc_core::phys::BlockHitResult,
        projectile: &mut dyn crate::world::entity::projectile::Projectile,
    ) {
        // `canBeLit`: `!WATERLOGGED && !LIT`
        abstract_candle_block::on_projectile_hit(
            level,
            state,
            hit,
            &*projectile,
            !state.get(props::WATERLOGGED) && !state.get(props::LIT),
        );
    }
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn Explosion,
        on_hit: &mut dyn FnMut(ItemStack, BlockPos),
    ) {
        abstract_candle_block::on_explosion_hit(state, level, pos, explosion, on_hit);
    }
}

register_block_class!("CandleBlock", |_| Box::new(CandleBlock));
