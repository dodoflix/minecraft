//! `net.minecraft.world.level.block.HangingMossBlock` (pale hanging moss).
use super::multiface_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::bonemeal::BonemealSource;

pub struct HangingMossBlock;

fn can_stay_at_position(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let neighbour_pos = pos.relative(Direction::Up);
    let block_state = level.get_block_state(neighbour_pos);
    multiface_block::can_attach_to(level, Direction::Up, neighbour_pos, block_state) || block_state.is_block(state.block())
}

/// `getTip`: the lowest block of the hanging moss column that contains `pos`.
pub fn get_tip(level: &dyn BlockGetter, pos: BlockPos) -> BlockPos {
    let mut forward_pos = pos;
    loop {
        forward_pos = forward_pos.below();
        if !level.get_block_state(forward_pos).is_block(Block::PALE_HANGING_MOSS) {
            break;
        }
    }
    forward_pos.above()
}

impl BlockBehaviour for HangingMossBlock {
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !can_stay_at_position(state, &*level, pos) {
            level.destroy_block(pos, true, None, 512);
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        let grow_pos = get_tip(level, pos).below();
        level.get_block_state(grow_pos).is_air() && !level.is_outside_build_height(grow_pos.y())
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        let tip_pos = get_tip(&*level, pos).below();
        if level.get_block_state(tip_pos).is_air() {
            level.set_block_and_update(tip_pos, state.set(props::TIP, true));
        }
    }
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
