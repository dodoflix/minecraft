//! `net.minecraft.world.level.block.SculkShriekerBlock`.
use super::prelude::*;

pub struct SculkShriekerBlock;

impl BlockBehaviour for SculkShriekerBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("SculkShriekerBlock", |_| Box::new(SculkShriekerBlock));
