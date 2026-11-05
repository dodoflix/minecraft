//! `net.minecraft.world.level.block.HangingMossBlock` (pale hanging moss).
use super::prelude::*;
use super::multiface_block;

pub struct HangingMossBlock;

fn can_stay_at_position(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let neighbour_pos = pos.relative(Direction::Up);
    let block_state = level.get_block_state(neighbour_pos);
    multiface_block::can_attach_to(level, Direction::Up, neighbour_pos, block_state) || block_state.is_block(state.block())
}

impl BlockBehaviour for HangingMossBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_stay_at_position(state, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !can_stay_at_position(state, &*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state.set(props::TIP, !level.get_block_state(pos.below()).is_block(state.block()))
    });
}

register_block_class!("HangingMossBlock", |_| Box::new(HangingMossBlock));
