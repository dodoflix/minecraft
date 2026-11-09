//! `ScaffoldingBlock.getDistance`, shared by the block and `ScaffoldingBlockItem.updatePlacementContext`.
use super::prelude::*;

/// `ScaffoldingBlock.getDistance`.
pub fn get_distance(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    let below_pos = pos.below();
    let below_state = level.get_block_state(below_pos);
    let mut distance = 7;
    if below_state.is_block(Block::SCAFFOLDING) {
        distance = below_state.get(props::STABILITY_DISTANCE);
    } else if below_state.is_face_sturdy_at(level, below_pos, Direction::Up) {
        return 0;
    }
    for direction in Direction::HORIZONTAL {
        let relative_state = level.get_block_state(pos.relative(direction));
        if relative_state.is_block(Block::SCAFFOLDING) {
            distance = distance.min(relative_state.get(props::STABILITY_DISTANCE) + 1);
            if distance == 1 {
                break;
            }
        }
    }
    distance
}
