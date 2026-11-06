//! `LandRandomPos`.

use super::goal_utils as gu;
use super::random_pos::{self, generate_random_pos, generate_random_pos_weighted};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::BlockGetter;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;

/// `getPos(mob, horizontalDist, verticalDist)`.
pub fn get_pos(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
) -> Option<Vec3> {
    get_pos_weighted(
        mob,
        level,
        horizontal_dist,
        vertical_dist,
        &mut |mob, level, pos| f64::from(mob.get_walk_target_value(pos, level)),
    )
}

/// `getPos(mob, horizontalDist, verticalDist, positionWeight)`.
pub fn get_pos_weighted(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
    position_weight: &mut dyn FnMut(&dyn Entity, &ServerLevel, BlockPos) -> f64,
) -> Option<Vec3> {
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    generate_random_pos_weighted(
        mob,
        level,
        &mut |mob, level| {
            let direction = random_pos::generate_random_direction(
                &mut mob.base_mut().random,
                horizontal_dist,
                vertical_dist,
            );
            let pos = generate_random_pos_toward_direction(
                mob,
                level,
                f64::from(horizontal_dist),
                restrict,
                direction,
            )?;
            move_pos_up_out_of_solid(mob, level, pos)
        },
        position_weight,
    )
}

/// `getPosTowards(mob, horizontalDist, verticalDist, towardsPos)`.
pub fn get_pos_towards(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
    towards_pos: Vec3,
) -> Option<Vec3> {
    let dir = towards_pos.subtract_xyz(mob.base().x(), mob.base().y(), mob.base().z());
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    get_pos_in_direction(
        mob,
        level,
        0.0,
        f64::from(horizontal_dist),
        vertical_dist,
        dir,
        restrict,
    )
}

/// `getPosAway(mob, horizontalDist, verticalDist, avoidPos)`.
pub fn get_pos_away(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
    avoid_pos: Vec3,
) -> Option<Vec3> {
    get_pos_away_range(
        mob,
        level,
        0.0,
        f64::from(horizontal_dist),
        vertical_dist,
        avoid_pos,
    )
}

/// `getPosAway(mob, minHorizontalDist, maxHorizontalDist, verticalDist, avoidPos)`.
pub fn get_pos_away_range(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    min_horizontal_dist: f64,
    max_horizontal_dist: f64,
    vertical_dist: i32,
    avoid_pos: Vec3,
) -> Option<Vec3> {
    let mut dir_away = mob.position().subtract(avoid_pos);
    if dir_away.length() == 0.0 {
        let random = &mut mob.base_mut().random;
        let x = random.next_double() - 0.5;
        let z = random.next_double() - 0.5;
        dir_away = Vec3::new(x, 0.0, z);
    }
    let restrict = gu::mob_restricted(mob, max_horizontal_dist);
    get_pos_in_direction(
        mob,
        level,
        min_horizontal_dist,
        max_horizontal_dist,
        vertical_dist,
        dir_away,
        restrict,
    )
}

fn get_pos_in_direction(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    min_horizontal_dist: f64,
    max_horizontal_dist: f64,
    vertical_dist: i32,
    dir: Vec3,
    restrict: bool,
) -> Option<Vec3> {
    generate_random_pos(mob, level, &mut |mob, level| {
        let direction = random_pos::generate_random_direction_within_radians(
            &mut mob.base_mut().random,
            min_horizontal_dist,
            max_horizontal_dist,
            vertical_dist,
            0,
            dir.x,
            dir.z,
            f64::from((std::f64::consts::PI / 2.0) as f32),
        )?;
        let pos = generate_random_pos_toward_direction(
            mob,
            level,
            max_horizontal_dist,
            restrict,
            direction,
        )?;
        move_pos_up_out_of_solid(mob, level, pos)
    })
}

/// `movePosUpOutOfSolid(mob, pos)`.
pub fn move_pos_up_out_of_solid(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    pos: BlockPos,
) -> Option<BlockPos> {
    let max_y = level.get_max_y();
    let pos = random_pos::move_up_out_of_solid(pos, max_y, |p| gu::is_solid(level, p));
    (!gu::is_water(level, pos) && !gu::has_malus(mob, level, pos)).then_some(pos)
}

/// `generateRandomPosTowardDirection(mob, horizontalDist, restrict, direction)`.
pub fn generate_random_pos_toward_direction(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: f64,
    restrict: bool,
    direction: BlockPos,
) -> Option<BlockPos> {
    let pos = random_pos::generate_random_pos_toward_direction(mob, horizontal_dist, direction);
    (!gu::is_outside_limits(level, pos)
        && !gu::is_restricted(restrict, mob, pos)
        && !gu::is_not_stable(mob, level, pos))
    .then_some(pos)
}
