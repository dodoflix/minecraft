//! `AirRandomPos`.

use super::air_and_water_random_pos;
use super::goal_utils as gu;
use super::random_pos::generate_random_pos;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;

/// `getPosTowards(mob, horizontalDist, verticalDist, flyingHeight, towardsPos, maxXzRadiansFromDir)`.
pub fn get_pos_towards(mob: &mut dyn Entity, level: &mut ServerLevel, horizontal_dist: i32, vertical_dist: i32, flying_height: i32, towards_pos: Vec3, max_xz_radians_from_dir: f64) -> Option<Vec3> {
    let dir = towards_pos.subtract_xyz(mob.base().x(), mob.base().y(), mob.base().z());
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    generate_random_pos(mob, level, &mut |mob, level| {
        let pos = air_and_water_random_pos::generate_random_pos(mob, level, horizontal_dist, vertical_dist, flying_height, dir.x, dir.z, max_xz_radians_from_dir, restrict)?;
        (!gu::is_water(level, pos)).then_some(pos)
    })
}
