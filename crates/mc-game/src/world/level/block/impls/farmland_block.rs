//! `net.minecraft.world.level.block.FarmlandBlock`.
use super::prelude::*;

pub struct FarmlandBlock;

impl BlockBehaviour for FarmlandBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = level.get_block_state(pos.above());
        !above_state.is_solid() || level.get_block_state(pos.above()).is_tag("maintains_farmland")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
}

register_block_class!("FarmlandBlock", |_| Box::new(FarmlandBlock));
