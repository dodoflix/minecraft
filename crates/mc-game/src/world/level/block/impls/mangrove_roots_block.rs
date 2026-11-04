//! `net.minecraft.world.level.block.MangroveRootsBlock`.
use super::prelude::*;

pub struct MangroveRootsBlock;

impl BlockBehaviour for MangroveRootsBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("MangroveRootsBlock", |_| Box::new(MangroveRootsBlock));
