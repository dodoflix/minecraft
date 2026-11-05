//! `net.minecraft.world.level.block.SculkSensorBlock`.
use super::prelude::*;

pub struct SculkSensorBlock;

impl BlockBehaviour for SculkSensorBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("SculkSensorBlock", |_| Box::new(SculkSensorBlock));
