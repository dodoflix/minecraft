//! `net.minecraft.world.level.block.ConduitBlock`.
use super::prelude::*;

pub struct ConduitBlock;

impl BlockBehaviour for ConduitBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("ConduitBlock", |_| Box::new(ConduitBlock));
