//! `net.minecraft.world.level.block.DoorBlock`.
use super::prelude::*;
use mc_data::enums::DoubleBlockHalf;

pub struct DoorBlock;

impl BlockBehaviour for DoorBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = pos.below();
        let below_state = level.get_block_state(below);
        if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower {
            below_state.is_face_sturdy_at(level, below, Direction::Up)
        } else {
            below_state.is_block(state.block())
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let half = state.get(props::DOUBLE_BLOCK_HALF);
        if direction.get_axis() != Axis::Y || (half == DoubleBlockHalf::Lower) != (direction == Direction::Up) {
            if half == DoubleBlockHalf::Lower && direction == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
        } else if helpers::is_instance_of(neighbour_state, "DoorBlock") && neighbour_state.get(props::DOUBLE_BLOCK_HALF) != half {
            neighbour_state.set(props::DOUBLE_BLOCK_HALF, half)
        } else {
            air()
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        if mirror == Mirror::None {
            state
        } else {
            state.rotate(mirror.get_rotation(state.get(props::HORIZONTAL_FACING))).cycle(props::DOOR_HINGE)
        }
    }
}

register_block_class!("DoorBlock", |_| Box::new(DoorBlock));
