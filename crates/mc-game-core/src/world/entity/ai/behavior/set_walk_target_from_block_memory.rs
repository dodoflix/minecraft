//! `SetWalkTargetFromBlockMemory` (a `Villager` behavior: it calls `releasePoi` on its body).

use super::declarative::BehaviorBuilder;
use super::OneShot;
use crate::world::entity::ai::memory::memory_module_type::{CANT_REACH_WALK_TARGET_SINCE, WALK_TARGET};
use crate::world::entity::ai::memory::{MemoryModuleType, WalkTarget};
use crate::world::entity::ai::util::default_random_pos;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;
use mc_protocol::codec::GlobalPos;

/// `create(memoryType, speedModifier, closeEnoughDist, tooFarDistance, tooLongUnreachableDuration)`.
pub fn create(memory_type: MemoryModuleType<GlobalPos>, speed_modifier: f32, close_enough_dist: i32, too_far_distance: i32, too_long_unreachable_duration: i32) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let cant_reach_since = i.registered(&CANT_REACH_WALK_TARGET_SINCE);
    let walk_target = i.absent(&WALK_TARGET);
    let memory = i.present(&memory_type);
    i.create(move |c| {
        let target_pos = c.get(&memory).clone();
        let cant_reach_target_since = c.try_get(&cant_reach_since).copied();
        let timestamp = c.timestamp;
        if target_pos.dimension == c.level.dimension && cant_reach_target_since.is_none_or(|since| c.level.game_time() - since <= i64::from(too_long_unreachable_duration)) {
            let body_pos = c.body.block_position();
            if target_pos.pos.dist_manhattan(&body_pos) > too_far_distance {
                let mut towards_target_pos: Option<Vec3> = None;
                let mut tries = 0;
                while towards_target_pos.is_none_or(|t| BlockPos::containing(t.x, t.y, t.z).dist_manhattan(&body_pos) > too_far_distance) {
                    towards_target_pos = default_random_pos::get_pos_towards(
                        &mut *c.body,
                        &mut *c.level,
                        15,
                        7,
                        Vec3::at_bottom_center_of(&target_pos.pos),
                        f64::from(std::f32::consts::FRAC_PI_2),
                    );
                    tries += 1;
                    if tries == 1000 {
                        c.body.release_poi(&mut *c.level, memory_type.id());
                        c.erase(&memory);
                        c.set(&cant_reach_since, timestamp);
                        return true;
                    }
                }
                if let Some(towards) = towards_target_pos {
                    c.set(&walk_target, WalkTarget::from_vec3(towards, speed_modifier, close_enough_dist));
                }
            } else if target_pos.pos.dist_manhattan(&body_pos) > close_enough_dist {
                c.set(&walk_target, WalkTarget::from_block_pos(target_pos.pos, speed_modifier, close_enough_dist));
            }
        } else {
            c.body.release_poi(&mut *c.level, memory_type.id());
            c.erase(&memory);
            c.set(&cant_reach_since, timestamp);
        }
        true
    })
}
