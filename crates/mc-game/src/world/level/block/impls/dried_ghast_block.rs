//! `net.minecraft.world.level.block.DriedGhastBlock`.
use super::prelude::*;

pub struct DriedGhastBlock;

impl BlockBehaviour for DriedGhastBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("DriedGhastBlock", |_| Box::new(DriedGhastBlock));
