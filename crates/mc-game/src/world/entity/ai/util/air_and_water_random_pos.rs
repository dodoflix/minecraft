//! `AirAndWaterRandomPos`.

use super::goal_utils as gu;
use super::random_pos;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::BlockGetter;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

/// `getPos(...)`.
#[allow(clippy::too_many_arguments)]
pub fn get_pos(mob: &mut dyn Entity, level: &mut ServerLevel, horizontal_dist: i32, vertical_dist: i32, flying_height: i32, x_dir: f64, z_dir: f64, max_xz_radians_difference: f64) -> Option<Vec3> {
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    random_pos::generate_random_pos(mob, level, &mut |mob, level| {
        generate_random_pos(mob, level, horizontal_dist, vertical_dist, flying_height, x_dir, z_dir, max_xz_radians_difference, restrict)
    })
}

/// `generateRandomPos(...)`.
#[allow(clippy::too_many_arguments)]
pub fn generate_random_pos(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
    flying_height: i32,
    x_dir: f64,
    z_dir: f64,
    max_xz_radians_difference: f64,
    restrict: bool,
) -> Option<BlockPos> {
    let direction = random_pos::generate_random_direction_within_radians(
        &mut mob.base_mut().random,
        0.0,
        f64::from(horizontal_dist),
        vertical_dist,
        flying_height,
        x_dir,
        z_dir,
        max_xz_radians_difference,
    )?;
    let pos = random_pos::generate_random_pos_toward_direction(mob, f64::from(horizontal_dist), direction);
    if !gu::is_outside_limits(level, pos) && !gu::is_restricted(restrict, mob, pos) {
        let max_y = level.get_max_y();
        let pos = random_pos::move_up_out_of_solid(pos, max_y, |p| gu::is_solid(level, p));
        if gu::has_malus(mob, level, pos) { None } else { Some(pos) }
    } else {
        None
    }
}
