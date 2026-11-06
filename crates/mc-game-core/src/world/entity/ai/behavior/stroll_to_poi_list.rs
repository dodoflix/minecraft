//! `StrollToPoiList`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::memory::{MemoryModuleType, WalkTarget};
use crate::world::level::LevelAccessor as _;
use mc_protocol::codec::GlobalPos;

/// `create(strollToMemoryType, speedModifier, closeEnoughDist, maxDistanceFromPoi, mustBeCloseToMemoryType)`.
pub fn create(
    stroll_to_memory_type: MemoryModuleType<Vec<GlobalPos>>,
    speed_modifier: f32,
    close_enough_dist: i32,
    max_distance_from_poi: i32,
    must_be_close_to_memory_type: MemoryModuleType<GlobalPos>,
) -> OneShot {
    let mut next_ok_start_time: i64 = 0;
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.registered(&WALK_TARGET);
    let stroll_to_memory = i.present(&stroll_to_memory_type);
    let must_be_close_to_memory = i.present(&must_be_close_to_memory_type);
    i.create(move |c| {
        let stroll_to = c.get(&stroll_to_memory).clone();
        let stay_close_to = c.get(&must_be_close_to_memory).clone();
        if stroll_to.is_empty() {
            return false;
        }
        let target_pos =
            stroll_to[c.level.random().next_int_bounded(stroll_to.len() as i32) as usize].clone();
        if c.level.dimension == target_pos.dimension
            && stay_close_to
                .pos
                .closer_to_center_than(&c.body().position(), f64::from(max_distance_from_poi))
        {
            if c.timestamp > next_ok_start_time {
                c.set(
                    &walk_target,
                    WalkTarget::from_block_pos(target_pos.pos, speed_modifier, close_enough_dist),
                );
                next_ok_start_time = c.timestamp + 100;
            }
            true
        } else {
            false
        }
    })
}
