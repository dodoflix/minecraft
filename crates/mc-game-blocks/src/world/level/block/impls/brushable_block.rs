//! `net.minecraft.world.level.block.BrushableBlock` (suspicious sand / gravel; extends `BaseEntityBlock`).
use super::prelude::*;

pub struct BrushableBlock;

impl BlockBehaviour for BrushableBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        helpers::schedule_block_tick(level, pos, state.block(), 2);
        state
    });
}

register_block_class!("BrushableBlock", |_| Box::new(BrushableBlock));
