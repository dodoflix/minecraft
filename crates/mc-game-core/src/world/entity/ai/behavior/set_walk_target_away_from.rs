//! `SetWalkTargetAwayFrom`.

use super::OneShot;
use super::declarative::BehaviorBuilder;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::memory::memory_module_type::WALK_TARGET;
use crate::world::entity::ai::memory::{EntityRef, MemoryData, MemoryModuleType, WalkTarget};
use crate::world::entity::ai::util::land_random_pos;
use mc_core::phys::Vec3;
use mc_core::pos::BlockPos;

/// `pos(memory, speedModifier, desiredDistance, interruptCurrentWalk)`.
pub fn pos(
    memory: MemoryModuleType<BlockPos>,
    speed_modifier: f32,
    desired_distance: i32,
    interrupt_current_walk: bool,
) -> OneShot {
    create(
        memory,
        speed_modifier,
        desired_distance,
        interrupt_current_walk,
        |_, pos: &BlockPos| Some(Vec3::at_bottom_center_of(pos)),
    )
}

/// `entity(memory, speedModifier, desiredDistance, interruptCurrentWalk)`.
pub fn entity(
    memory: MemoryModuleType<EntityRef>,
    speed_modifier: f32,
    desired_distance: i32,
    interrupt_current_walk: bool,
) -> OneShot {
    create(
        memory,
        speed_modifier,
        desired_distance,
        interrupt_current_walk,
        |level, e: &EntityRef| {
            level
                .get_entity_or_recently_removed(e.0)
                .map(|e| e.position())
        },
    )
}

fn create<T: MemoryData>(
    walk_away_from_memory: MemoryModuleType<T>,
    speed_modifier: f32,
    desired_distance: i32,
    interrupt_current_walk: bool,
    to_position: impl Fn(&ServerLevel, &T) -> Option<Vec3> + 'static,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.registered(&WALK_TARGET);
    let walk_away_from = i.present(&walk_away_from_memory);
    i.create(move |c| {
        let target = c.try_get(&walk_target).cloned();
        if target.is_some() && !interrupt_current_walk {
            return false;
        }
        let body_position = c.body.position();
        let Some(avoid_position) = to_position(&*c.level, c.get(&walk_away_from)) else {
            return false;
        };
        if !body_position.closer_than(avoid_position, f64::from(desired_distance)) {
            return false;
        }
        if let Some(target) = &target
            && target.get_speed_modifier() == speed_modifier
        {
            let current_direction = target
                .get_target()
                .current_position(&*c.level)
                .subtract(body_position);
            let avoid_direction = avoid_position.subtract(body_position);
            if current_direction.dot(avoid_direction) < 0.0 {
                return false;
            }
        }
        for _ in 0..10 {
            if let Some(flee_to_pos) =
                land_random_pos::get_pos_away(&mut *c.body, &mut *c.level, 16, 7, avoid_position)
            {
                c.set(
                    &walk_target,
                    WalkTarget::from_vec3(flee_to_pos, speed_modifier, 0),
                );
                break;
            }
        }
        true
    })
}
