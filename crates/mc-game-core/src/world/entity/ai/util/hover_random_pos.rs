//! `HoverRandomPos`.

use super::goal_utils as gu;
use super::land_random_pos;
use super::random_pos::{self, generate_random_pos};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::level::BlockGetter;
use mc_core::phys::Vec3;
use mc_core::random::RandomSource;

/// `getPos(...)`.
#[allow(clippy::too_many_arguments)]
pub fn get_pos(
    mob: &mut dyn Entity,
    level: &mut ServerLevel,
    horizontal_dist: i32,
    vertical_dist: i32,
    x_dir: f64,
    z_dir: f64,
    max_xz_radians_difference: f32,
    hover_max_height: i32,
    hover_min_height: i32,
) -> Option<Vec3> {
    let restrict = gu::mob_restricted(mob, f64::from(horizontal_dist));
    generate_random_pos(mob, level, &mut |mob, level| {
        let direction = random_pos::generate_random_direction_within_radians(
            &mut mob.base_mut().random,
            0.0,
            f64::from(horizontal_dist),
            vertical_dist,
            0,
            x_dir,
            z_dir,
            f64::from(max_xz_radians_difference),
        )?;
        let pos = land_random_pos::generate_random_pos_toward_direction(mob, level, f64::from(horizontal_dist), restrict, direction)?;
        let above = mob.base_mut().random.next_int_bounded(hover_max_height - hover_min_height + 1) + hover_min_height;
        let max_y = level.get_max_y();
        let pos = random_pos::move_up_to_above_solid(pos, above, max_y, |p| gu::is_solid(level, p));
        (!gu::is_water(level, pos) && !gu::has_malus(mob, level, pos)).then_some(pos)
    })
}
