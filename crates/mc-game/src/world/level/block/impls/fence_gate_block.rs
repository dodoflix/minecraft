//! `net.minecraft.world.level.block.FenceGateBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;

pub struct FenceGateBlock;

/// `FenceGateBlock.connectsToDirection`.
pub fn connects_to_direction(state: BlockState, direction: Direction) -> bool {
    state.get(props::HORIZONTAL_FACING).get_axis() == direction.get_clockwise().get_axis()
}

fn is_wall(state: BlockState) -> bool {
    state.is_tag("walls")
}

impl BlockBehaviour for FenceGateBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let axis = direction.get_axis();
        if state.get(props::HORIZONTAL_FACING).get_clockwise().get_axis() != axis {
            state
        } else {
            let in_wall = is_wall(neighbour_state) || is_wall(level.get_block_state(pos.relative(direction.get_opposite())));
            state.set(props::IN_WALL, in_wall)
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("FenceGateBlock", |_| Box::new(FenceGateBlock));
