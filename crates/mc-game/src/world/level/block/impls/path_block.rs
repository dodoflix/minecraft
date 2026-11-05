//! `net.minecraft.world.level.block.PathBlock` (dirt path).
use super::prelude::*;

pub struct PathBlock;

impl BlockBehaviour for PathBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = level.get_block_state(pos.above());
        !above_state.is_solid() || helpers::is_instance_of(above_state, "FenceGateBlock")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
}

register_block_class!("PathBlock", |_| Box::new(PathBlock));
