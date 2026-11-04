//! Static helpers of `net.minecraft.world.level.block.Block`.

use super::BlockStateExt;
use super::state::UPDATE_SHAPE_ORDER;
use crate::world::level::LevelAccessor;
use crate::world::level::flags::{UPDATE_SUPPRESS_DROPS};
use crate::world::level::shapes::is_shape_full_block;
use mc_core::direction::Direction;
use mc_core::phys::shapes::VoxelShape;
use mc_core::pos::BlockPos;
use mc_data::block::{BlockState, SupportType};

pub use crate::world::level::shapes::{box_ as box_shape, is_face_full};

/// `Block.isShapeFullBlock`.
pub fn is_shape_full(shape: &VoxelShape) -> bool {
    is_shape_full_block(shape)
}

/// `Block.updateFromNeighbourShapes`.
pub fn update_from_neighbour_shapes(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) -> BlockState {
    let mut new_state = state;
    for direction in UPDATE_SHAPE_ORDER {
        let neighbour_pos = pos.relative(direction);
        let neighbour_state = level.get_block_state(neighbour_pos);
        new_state = new_state.update_shape(level, pos, direction, neighbour_pos, neighbour_state);
    }
    new_state
}

/// `Block.updateOrDestroy(old, new, level, pos, flags)` (limit 512).
pub fn update_or_destroy(block_state: BlockState, new_state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, update_flags: u32) {
    update_or_destroy_limited(block_state, new_state, level, pos, update_flags, 512);
}

/// `Block.updateOrDestroy(old, new, level, pos, flags, updateLimit)`.
pub fn update_or_destroy_limited(
    block_state: BlockState,
    new_state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    update_flags: u32,
    update_limit: i32,
) {
    if new_state != block_state {
        if new_state.is_air() {
            level.destroy_block(pos, update_flags & UPDATE_SUPPRESS_DROPS == 0, None, update_limit);
        } else {
            level.set_block(pos, new_state, update_flags & !UPDATE_SUPPRESS_DROPS, update_limit);
        }
    }
}

/// `Block.canSupportRigidBlock(level, below)`.
pub fn can_support_rigid_block(level: &dyn crate::world::level::BlockGetter, below: BlockPos) -> bool {
    level.get_block_state(below).is_face_sturdy_support(level, below, Direction::Up, SupportType::Rigid)
}

/// `Block.canSupportCenter(level, belowPos, direction)`.
pub fn can_support_center(level: &dyn crate::world::level::BlockGetter, below_pos: BlockPos, direction: Direction) -> bool {
    let state = level.get_block_state(below_pos);
    if direction == Direction::Down && state.is_tag("unstable_bottom_center") {
        false
    } else {
        state.is_face_sturdy_support(level, below_pos, direction, SupportType::Center)
    }
}

/// `Block.getStateForPlacement(context)` of `block`.
pub fn get_state_for_placement(block: mc_data::block::Block, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
    super::behaviour(block).get_state_for_placement(block, context)
}
