//! `net.minecraft.world.level.block.BarrierBlock` (`SimpleWaterloggedBlock`, creative players only).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::entity::Entity;
use crate::world::item::ItemStack;
use crate::world::level::block::simple_waterlogged;
use mc_data::fluid::{Fluid, FluidState};

/// `user instanceof Player player && player.isCreative()`.
fn is_creative_player(user: Option<&dyn Entity>) -> bool {
    user.and_then(|e| e.player()).is_some_and(|p| p.abilities.instabuild)
}

pub struct BarrierBlock;

impl BlockBehaviour for BarrierBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::WATERLOGGED, level.get_fluid_state(context.get_clicked_pos()).get_type() == Fluid::WATER))
    }
    fn is_liquid_block_container(&self) -> bool {
        true
    }
    /// Only creative players can waterlog a barrier.
    fn can_place_liquid(&self, user: Option<&dyn Entity>, level: &dyn BlockGetter, pos: BlockPos, state: BlockState, fluid: Fluid) -> bool {
        is_creative_player(user) && simple_waterlogged::can_place_liquid(user, level, pos, state, fluid)
    }
    fn place_liquid(&self, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState, fluid_state: FluidState) -> bool {
        simple_waterlogged::place_liquid(level, pos, state, fluid_state)
    }
    fn is_bucket_pickup(&self) -> bool {
        true
    }
    fn pickup_block(&self, user: Option<&dyn Entity>, level: &mut dyn LevelAccessor, pos: BlockPos, state: BlockState) -> ItemStack {
        if is_creative_player(user) { simple_waterlogged::pickup_block(user, level, pos, state) } else { ItemStack::empty() }
    }
    fn get_pickup_sound(&self) -> Option<mc_data::SoundEvent> {
        simple_waterlogged::get_pickup_sound()
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("BarrierBlock", |_| Box::new(BarrierBlock));
