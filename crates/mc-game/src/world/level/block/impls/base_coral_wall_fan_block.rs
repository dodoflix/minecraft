//! `net.minecraft.world.level.block.BaseCoralWallFanBlock` (extends `BaseCoralFanBlock`).
use super::prelude::*;

pub struct BaseCoralWallFanBlock;

/// `BaseCoralWallFanBlock.canSurvive`.
pub fn can_survive(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    let facing = state.get(props::HORIZONTAL_FACING);
    let relative_pos = pos.relative(facing.get_opposite());
    level.get_block_state(relative_pos).is_face_sturdy_at(level, relative_pos, facing)
}

/// `BaseCoralWallFanBlock.updateShape`.
pub fn update_shape(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, direction_to_neighbour: Direction) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    if direction_to_neighbour.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) { air() } else { state }
}

impl BlockBehaviour for BaseCoralWallFanBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive(state, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        update_shape(state, level, pos, direction)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BaseCoralWallFanBlock", |_| Box::new(BaseCoralWallFanBlock));
