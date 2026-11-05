//! `net.minecraft.world.level.block.HeavyCoreBlock`.
use super::prelude::*;

pub struct HeavyCoreBlock;

impl BlockBehaviour for HeavyCoreBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("HeavyCoreBlock", |_| Box::new(HeavyCoreBlock));
