//! `net.minecraft.world.level.block.BarrierBlock`.
use super::prelude::*;

pub struct BarrierBlock;

impl BlockBehaviour for BarrierBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("BarrierBlock", |_| Box::new(BarrierBlock));
