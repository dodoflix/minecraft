//! `StrollToPoi`.

use super::{BehaviorBuilder, OneShot};
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::memory::{MemoryModuleType, WalkTarget};
use mc_protocol::codec::GlobalPos;

/// `create(memoryType, speedModifier, closeEnoughDist, maxDistanceFromPoi)`.
pub fn create(memory_type: MemoryModuleType<GlobalPos>, speed_modifier: f32, close_enough_dist: i32, max_distance_from_poi: i32) -> OneShot {
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
            c.set(&walk_target, WalkTarget::from_block_pos(pos.pos, speed_modifier, close_enough_dist));
            next_ok_start_time = c.timestamp + 80;
            true
        }
    })
}
