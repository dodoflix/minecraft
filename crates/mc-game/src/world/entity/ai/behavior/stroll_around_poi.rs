//! `StrollAroundPoi`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::memory::{MemoryModuleType, WalkTarget};
use crate::world::entity::ai::util::land_random_pos;
use mc_protocol::codec::GlobalPos;

const MIN_TIME_BETWEEN_STROLLS: i64 = 180;
const STROLL_MAX_XZ_DIST: i32 = 8;
const STROLL_MAX_Y_DIST: i32 = 6;

/// `create(memoryType, speedModifier, maxDistanceFromPoi)`.
pub fn create(memory_type: MemoryModuleType<GlobalPos>, speed_modifier: f32, max_distance_from_poi: i32) -> OneShot {
    let mut next_ok_start_time: i64 = 0;
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.registered(&WALK_TARGET);
    let memory = i.present(&memory_type);
    i.create(move |c| {
        let pos = c.get(&memory).clone();
        if c.level.dimension != pos.dimension || !pos.pos.closer_to_center_than(&c.body().position(), f64::from(max_distance_from_poi)) {
            false
        } else if c.timestamp <= next_ok_start_time {
            true
        } else {
            let land_pos = land_random_pos::get_pos(c.body, c.level, STROLL_MAX_XZ_DIST, STROLL_MAX_Y_DIST);
            c.set_or_erase(&walk_target, land_pos.map(|p| WalkTarget::from_vec3(p, speed_modifier, 1)));
            next_ok_start_time = c.timestamp + MIN_TIME_BETWEEN_STROLLS;
            true
        }
    })
}
