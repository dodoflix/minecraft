//! `net.minecraft.world.level.block.WaterloggedTransparentBlock` (`TransparentBlock` subclass).
use super::prelude::*;

pub struct WaterloggedTransparentBlock;

impl BlockBehaviour for WaterloggedTransparentBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("WaterloggedTransparentBlock", |_| Box::new(WaterloggedTransparentBlock));
