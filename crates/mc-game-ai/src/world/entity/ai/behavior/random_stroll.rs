//! `RandomStroll`.

use super::behavior_utils;
use super::declarative::BehaviorBuilder;
use super::OneShot;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::util::{air_and_water_random_pos, goal_utils, land_random_pos};
use crate::world::entity::entity::Entity;
use crate::world::level::BlockGetter;
use crate::world::level::material::FluidStateExt;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

const MAX_XZ_DIST: i32 = 10;
const MAX_Y_DIST: i32 = 7;
const SWIM_XY_DISTANCE_TIERS: [[i32; 2]; 6] = [[1, 1], [3, 3], [5, 5], [6, 5], [7, 7], [10, 7]];

/// `stroll(speedModifier)`.
pub fn stroll(speed_modifier: f32) -> OneShot {
    stroll_may_leave_water(speed_modifier, true)
}

/// `stroll(speedModifier, mayStrollFromWater)`.
pub fn stroll_may_leave_water(speed_modifier: f32, may_stroll_from_water: bool) -> OneShot {
    stroll_fly_or_swim(
        speed_modifier,
        |level, body| land_random_pos::get_pos(body, level, MAX_XZ_DIST, MAX_Y_DIST),
        move |body| may_stroll_from_water || !body.is_in_water(),
    )
}

/// `stroll(speedModifier, maxHorizontalDistance, maxVerticalDistance)`.
pub fn stroll_within(speed_modifier: f32, max_horizontal_distance: i32, max_vertical_distance: i32) -> OneShot {
    stroll_fly_or_swim(speed_modifier, move |level, body| land_random_pos::get_pos(body, level, max_horizontal_distance, max_vertical_distance), |_| true)
}

/// `fly(speedModifier)`.
pub fn fly(speed_modifier: f32) -> OneShot {
    stroll_fly_or_swim(speed_modifier, |level, body| get_target_fly_pos(level, body, MAX_XZ_DIST, MAX_Y_DIST), |_| true)
}

/// `swim(speedModifier)`.
pub fn swim(speed_modifier: f32) -> OneShot {
    stroll_fly_or_swim(speed_modifier, get_target_swim_pos, |body| body.is_in_water())
}

fn stroll_fly_or_swim(
    speed_modifier: f32,
    mut fetch_target_pos: impl FnMut(&mut ServerLevel, &mut dyn Entity) -> Option<Vec3> + 'static,
    can_run: impl Fn(&dyn Entity) -> bool + 'static,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    i.create(move |c| {
        if !can_run(&*c.body) {
            return false;
        }
        let path_goal_pos = fetch_target_pos(&mut *c.level, &mut *c.body);
        c.set_or_erase(&walk_target, path_goal_pos.map(|pos| WalkTarget::from_vec3(pos, speed_modifier, 0)));
        true
    })
}

fn get_target_swim_pos(level: &mut ServerLevel, body: &mut dyn Entity) -> Option<Vec3> {
    let mut fallback: Option<Vec3> = None;
    let mut target_pos: Option<Vec3> = None;
    for distance in SWIM_XY_DISTANCE_TIERS {
        target_pos = match fallback {
            None => behavior_utils::get_random_swimmable_pos(body, level, distance[0], distance[1]),
            Some(fallback) => {
                let position = body.position();
                Some(position.add(position.vector_to(fallback).normalize().multiply_xyz(f64::from(distance[0]), f64::from(distance[1]), f64::from(distance[0]))))
            }
        };
        let restrict = goal_utils::mob_restricted(body, f64::from(distance[0]));
        let Some(pos) = target_pos else { return fallback };
        if level.get_fluid_state(BlockPos::containing(pos.x, pos.y, pos.z)).is_empty() || goal_utils::is_restricted_vec(restrict, body, pos) {
            return fallback;
        }
        fallback = Some(pos);
    }
    target_pos
}

fn get_target_fly_pos(level: &mut ServerLevel, body: &mut dyn Entity, max_horizontal_distance: i32, max_vertical_distance: i32) -> Option<Vec3> {
    let wander_direction = body.get_view_vector(0.0);
    air_and_water_random_pos::get_pos(
        body,
        level,
        max_horizontal_distance,
        max_vertical_distance,
        -2,
        wander_direction.x,
        wander_direction.z,
        f64::from(std::f32::consts::FRAC_PI_2),
    )
}
