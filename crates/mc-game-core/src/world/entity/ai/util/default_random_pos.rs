//! `DefaultRandomPos`.

use super::goal_utils as gu;
use super::random_pos::{self, generate_random_pos};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

/// `getPos(mob, horizontalDist, verticalDist)`.
pub fn get_pos(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
) -> Option<Vec3> {
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    generate_random_pos(mob, level, &mut |mob, level| {
        let direction = random_pos::generate_random_direction(
            &mut mob.base_mut().random,
            horizontal_dist,
            vertical_dist,
        );
        generate_random_pos_toward_direction(mob, level, horizontal_dist, restrict, direction)
    })
}

/// `getPosTowards(mob, horizontalDist, verticalDist, towardsPos, maxXzRadiansFromDir)`.
pub fn get_pos_towards(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
    towards_pos: Vec3,
    max_xz_radians_from_dir: f64,
) -> Option<Vec3> {
    let dir = towards_pos.subtract_xyz(mob.base().x(), mob.base().y(), mob.base().z());
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    generate_random_pos(mob, level, &mut |mob, level| {
        let direction = random_pos::generate_random_direction_within_radians(
            &mut mob.base_mut().random,
            0.0,
            f64::from(horizontal_dist),
            vertical_dist,
            0,
            dir.x,
            dir.z,
            max_xz_radians_from_dir,
        )?;
        generate_random_pos_toward_direction(mob, level, horizontal_dist, restrict, direction)
    })
}

/// `getPosAway(mob, horizontalDist, verticalDist, avoidPos)`.
pub fn get_pos_away(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
    avoid_pos: Vec3,
) -> Option<Vec3> {
    let dir_away = mob.position().subtract(avoid_pos);
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    generate_random_pos(mob, level, &mut |mob, level| {
        let direction = random_pos::generate_random_direction_within_radians(
            &mut mob.base_mut().random,
            0.0,
            f64::from(horizontal_dist),
            vertical_dist,
            0,
            dir_away.x,
            dir_away.z,
            f64::from((std::f64::consts::PI / 2.0) as f32),
        )?;
        generate_random_pos_toward_direction(mob, level, horizontal_dist, restrict, direction)
    })
}

fn generate_random_pos_toward_direction(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    restrict: bool,
    direction: BlockPos,
) -> Option<BlockPos> {
    let pos = random_pos::generate_random_pos_toward_direction(
        mob,
        f64::from(horizontal_dist),
        direction,
    );
    (!gu::is_outside_limits(level, pos)
        && !gu::is_restricted(restrict, mob, pos)
        && !gu::is_not_stable(mob, level, pos)
        && !gu::has_malus(mob, level, pos))
    .then_some(pos)
}
