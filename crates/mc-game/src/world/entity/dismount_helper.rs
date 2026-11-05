//! `net.minecraft.world.entity.vehicle.DismountHelper` (the parts bed wake-up and vehicle dismount share) and
//! `EntityType.isBlockDangerous`.

use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::block::helpers;
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::collision::{BlockGetterExt, CollisionGetter};
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::{EntityType, props};

/// `NodeEvaluator.isBurningBlock(state)`.
fn is_burning_block(state: BlockState) -> bool {
    let block = state.block();
    block.is_in_tag("fire")
        || block == Block::LAVA
        || block == Block::MAGMA_BLOCK
        || (helpers::is_instance_of(state, "CampfireBlock") && state.get(props::LIT))
        || block == Block::LAVA_CAULDRON
}

/// `EntityType.isBlockDangerous(state)`.
pub fn is_block_dangerous(entity_type: EntityType, state: BlockState) -> bool {
    let data = entity_type.data();
    let immune_to = data.immune_to.strip_prefix("minecraft:").unwrap_or(data.immune_to);
    if state.block().is_in_tag(immune_to) {
        false
    } else if !data.fire_immune && is_burning_block(state) {
        true
    } else {
        let block = state.block();
        block == Block::WITHER_ROSE || block == Block::SWEET_BERRY_BUSH || block == Block::CACTUS || block == Block::POWDER_SNOW
    }
}

/// `isBlockFloorValid(blockFloorHeight)`.
pub fn is_block_floor_valid(block_floor_height: f64) -> bool {
    !block_floor_height.is_infinite() && block_floor_height < 1.0
}

/// `nonClimbableShape(level, pos)`.
pub fn non_climbable_shape(level: &dyn BlockGetter, pos: BlockPos) -> VoxelShape {
    let state = level.get_block_state(pos);
    if !state.block().is_in_tag("climbable") && (!helpers::is_instance_of(state, "TrapDoorBlock") || !state.get(props::OPEN)) {
        state.get_collision_shape(level, pos)
    } else {
        Shapes::empty()
    }
}

/// `findSafeDismountLocation(type, level, blockPos, checkDangerous)`.
pub fn find_safe_dismount_location(entity_type: EntityType, level: &ServerLevel, block_pos: BlockPos, check_dangerous: bool) -> Option<Vec3> {
    if check_dangerous && is_block_dangerous(entity_type, level.get_block_state(block_pos)) {
        return None;
    }
    let floor_height = ServerLevel::get_block_floor_height_of(&non_climbable_shape(level, block_pos), || non_climbable_shape(level, block_pos.below()));
    if !is_block_floor_valid(floor_height) {
        return None;
    }
    if check_dangerous && floor_height <= 0.0 && is_block_dangerous(entity_type, level.get_block_state(block_pos.below())) {
        return None;
    }
    let position = Vec3::up_from_bottom_center_of(&block_pos, floor_height);
    let aabb: AABB = crate::world::entity::EntityDimensions::of_type(entity_type).make_bounding_box_vec(position);
    if level.get_block_collisions(None, &aabb).iter().any(|shape| !shape.is_empty()) {
        return None;
    }
    if entity_type != EntityType::PLAYER
        || !level.get_block_state(block_pos).block().is_in_tag("invalid_spawn_inside") && !level.get_block_state(block_pos.above()).block().is_in_tag("invalid_spawn_inside")
    {
        if !level.world_border().is_within_bounds_aabb(&aabb) { None } else { Some(position) }
    } else {
        None
    }
}
