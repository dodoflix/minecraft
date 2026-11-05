//! `net.minecraft.world.level.block.LeavesBlock` (also the tinted / untinted particle variants).
use super::prelude::*;

pub struct LeavesBlock;

/// `LeavesBlock.getOptionalDistanceAt`.
pub fn get_optional_distance_at(state: BlockState) -> Option<i32> {
    if state.is_tag("prevents_nearby_leaf_decay") { Some(0) } else { state.try_get(props::DISTANCE) }
}

fn get_distance_at(state: BlockState) -> i32 {
    get_optional_distance_at(state).unwrap_or(7)
}

/// `LeavesBlock.updateDistance`.
pub fn update_distance(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    let mut new_distance = 7;
    for direction in Direction::ALL {
        new_distance = new_distance.min(get_distance_at(level.get_block_state(pos.relative(direction))) + 1);
        if new_distance == 1 {
            break;
        }
    }
    state.set(props::DISTANCE, new_distance)
}

impl BlockBehaviour for LeavesBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        let distance_from_neighbor = get_distance_at(neighbour_state) + 1;
        if distance_from_neighbor != 1 || state.get(props::DISTANCE) != distance_from_neighbor {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        state
    });
}

register_block_class!("LeavesBlock", |_| Box::new(LeavesBlock));
