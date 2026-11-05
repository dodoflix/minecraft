//! `GoalUtils`.

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob::navigation;
use crate::world::level::{BlockGetter, LevelHeight};
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

/// `hasGroundPathNavigation(mob)`.
pub fn has_ground_path_navigation(mob: &dyn Entity, level: &ServerLevel) -> bool {
    navigation::can_navigate_ground(mob, level)
}

/// `mobRestricted(mob, horizontalDist)`.
pub fn mob_restricted(mob: &dyn Entity, horizontal_dist: f64) -> bool {
    mob.mob().is_some_and(|m| {
        m.has_home() && m.home_position.closer_to_center_than(&mob.position(), f64::from(m.home_radius) + horizontal_dist + 1.0)
    })
}

/// `isOutsideLimits(pos, mob)`.
pub fn is_outside_limits(level: &ServerLevel, pos: BlockPos) -> bool {
    level.is_outside_build_height(pos.y())
}

/// `isRestricted(restrict, mob, pos)`.
pub fn is_restricted(restrict: bool, mob: &dyn Entity, pos: BlockPos) -> bool {
    restrict && !mob.mob().is_some_and(|m| m.is_within_home_pos(pos))
}

/// `isRestricted(restrict, mob, Vec3)`.
pub fn is_restricted_vec(restrict: bool, mob: &dyn Entity, pos: Vec3) -> bool {
    restrict && !mob.mob().is_some_and(|m| m.is_within_home_vec(pos))
}

/// `isNotStable(navigation, pos)`.
pub fn is_not_stable(mob: &dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
    !navigation::is_stable_destination(mob, level, pos)
}

/// `isWater(mob, pos)`.
pub fn is_water(level: &ServerLevel, pos: BlockPos) -> bool {
    mc_data::tags::get::<mc_data::fluid::Fluid>("water").is_some_and(|t| t.contains(level.get_fluid_state(pos).fluid()))
}

/// `hasMalus(mob, pos)`.
pub fn has_malus(mob: &mut dyn Entity, level: &ServerLevel, pos: BlockPos) -> bool {
    let ty = navigation::path_type_static(mob, level, pos);
    crate::world::entity::mob::get_pathfinding_malus(&*mob, level, ty) != 0.0
}

/// `isSolid(mob, pos)`.
pub fn is_solid(level: &ServerLevel, pos: BlockPos) -> bool {
    level.get_block_state(pos).is_solid()
}

#[allow(dead_code)]
fn _height(_: LevelHeight) {}
