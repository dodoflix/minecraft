//! `net.minecraft.world.level.block.LightBlock`.
use super::prelude::*;

pub struct LightBlock;

impl BlockBehaviour for LightBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("LightBlock", |_| Box::new(LightBlock));
