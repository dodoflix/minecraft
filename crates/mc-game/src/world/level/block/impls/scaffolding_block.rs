//! `net.minecraft.world.level.block.ScaffoldingBlock`.
use super::prelude::*;

pub struct ScaffoldingBlock;

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

impl BlockBehaviour for ScaffoldingBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        get_distance(level, pos) < 7
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        // `!level.isClientSide()` always holds on the server
        helpers::schedule_block_tick(level, pos, state.block(), 1);
        state
    });
}

register_block_class!("ScaffoldingBlock", |_| Box::new(ScaffoldingBlock));
