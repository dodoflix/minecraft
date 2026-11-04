//! `net.minecraft.world.level.block.StairBlock`.
use super::prelude::*;
use mc_data::enums::StairsShape;

pub struct StairBlock;

/// `StairBlock.isStairs`.
pub fn is_stairs(state: BlockState) -> bool {
    helpers::is_instance_of(state, "StairBlock")
}

fn can_take_shape(state: BlockState, level: &dyn BlockGetter, pos: BlockPos, neighbour: Direction) -> bool {
    let neighbor_state = level.get_block_state(pos.relative(neighbour));
    !is_stairs(neighbor_state)
        || neighbor_state.get(props::HORIZONTAL_FACING) != state.get(props::HORIZONTAL_FACING)
        || neighbor_state.get(props::HALF) != state.get(props::HALF)
}

/// `StairBlock.getStairsShape`.
pub fn get_stairs_shape(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> StairsShape {
    let facing = state.get(props::HORIZONTAL_FACING);
    let behind_state = level.get_block_state(pos.relative(facing));
    if is_stairs(behind_state) && state.get(props::HALF) == behind_state.get(props::HALF) {
        let behind_facing = behind_state.get(props::HORIZONTAL_FACING);
        if behind_facing.get_axis() != state.get(props::HORIZONTAL_FACING).get_axis()
            && can_take_shape(state, level, pos, behind_facing.get_opposite())
        {
            if behind_facing == facing.get_counter_clockwise() {
                return StairsShape::OuterLeft;
            }
            return StairsShape::OuterRight;
        }
    }
    let front_state = level.get_block_state(pos.relative(facing.get_opposite()));
    if is_stairs(front_state) && state.get(props::HALF) == front_state.get(props::HALF) {
        let front_facing = front_state.get(props::HORIZONTAL_FACING);
        if front_facing.get_axis() != state.get(props::HORIZONTAL_FACING).get_axis() && can_take_shape(state, level, pos, front_facing) {
            if front_facing == facing.get_counter_clockwise() {
                return StairsShape::InnerLeft;
            }
            return StairsShape::InnerRight;
        }
    }
    StairsShape::Straight
}

impl BlockBehaviour for StairBlock {
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction_to_neighbour.get_axis().is_horizontal() {
            state.set(props::STAIRS_SHAPE, get_stairs_shape(state, &*level, pos))
        } else {
            state
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        let direction = state.get(props::HORIZONTAL_FACING);
        let shape = state.get(props::STAIRS_SHAPE);
        let rotated = state.rotate(Rotation::Clockwise180);
        match mirror {
            Mirror::LeftRight if direction.get_axis() == Axis::Z => match shape {
                StairsShape::OuterLeft => rotated.set(props::STAIRS_SHAPE, StairsShape::OuterRight),
                StairsShape::InnerRight => rotated.set(props::STAIRS_SHAPE, StairsShape::InnerLeft),
                StairsShape::InnerLeft => rotated.set(props::STAIRS_SHAPE, StairsShape::InnerRight),
                StairsShape::OuterRight => rotated.set(props::STAIRS_SHAPE, StairsShape::OuterLeft),
                _ => rotated,
            },
            Mirror::FrontBack if direction.get_axis() == Axis::X => match shape {
                StairsShape::Straight => rotated,
                StairsShape::OuterLeft => rotated.set(props::STAIRS_SHAPE, StairsShape::OuterRight),
                StairsShape::InnerRight => rotated.set(props::STAIRS_SHAPE, StairsShape::InnerRight),
                StairsShape::InnerLeft => rotated.set(props::STAIRS_SHAPE, StairsShape::InnerLeft),
                StairsShape::OuterRight => rotated.set(props::STAIRS_SHAPE, StairsShape::OuterLeft),
            },
            _ => state,
        }
    }
}

register_block_class!("StairBlock", |_| Box::new(StairBlock));
