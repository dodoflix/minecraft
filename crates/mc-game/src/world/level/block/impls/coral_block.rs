//! `net.minecraft.world.level.block.CoralBlock`.
use super::prelude::*;

pub struct CoralBlock;

fn scan_for_water(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    Direction::ALL.into_iter().any(|direction| level.get_fluid_state(pos.relative(direction)).is_tag("water"))
}

impl BlockBehaviour for CoralBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !scan_for_water(&*level, pos) {
            let delay = 60 + level.random().next_int_bounded(40);
            helpers::schedule_block_tick(level, pos, state.block(), delay);
        }
        state
    });
}

register_block_class!("CoralBlock", |_| Box::new(CoralBlock));
