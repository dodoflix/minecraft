//! `net.minecraft.world.level.block.BarrierBlock` (`SimpleWaterloggedBlock`, creative players only).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use mc_data::fluid::Fluid;

pub struct BarrierBlock;

impl BlockBehaviour for BarrierBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::WATERLOGGED, level.get_fluid_state(context.get_clicked_pos()).get_type() == Fluid::WATER))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("BarrierBlock", |_| Box::new(BarrierBlock));
