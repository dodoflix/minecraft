//! `BabyFollowAdult`.

use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::world::entity::ai::memory::memory_module_type::{LOOK_TARGET, NEAREST_VISIBLE_ADULT, WALK_TARGET};
use mc_game_core::world::entity::ai::memory::{EntityRef, MemoryModuleType, WalkTarget};
use mc_game_core::world::entity::ai::sensing::closer_than;
use mc_game_core::world::entity::entity::Entity;

/// `create(followRange, speedModifier)`; `followRange` = the `UniformInt` bounds.
pub fn create(follow_range_min_inclusive: i32, follow_range_max_inclusive: i32, speed_modifier: f32) -> OneShot {
    create_with(follow_range_min_inclusive, follow_range_max_inclusive, move |_| speed_modifier, NEAREST_VISIBLE_ADULT, false)
}

/// `create(followRange, speedModifier, nearestVisibleType, targetEye)`.
pub fn create_with(
    follow_range_min_inclusive: i32,
    follow_range_max_inclusive: i32,
    speed_modifier: impl Fn(&dyn Entity) -> f32 + 'static,
    nearest_visible_type: MemoryModuleType<EntityRef>,
    target_eye: bool,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let nearest_adult = i.present(&nearest_visible_type);
    let look_target = i.registered(&LOOK_TARGET);
    let walk_target = i.absent(&WALK_TARGET);
    i.create(move |c| {
        if !c.body.is_baby() {
            return false;
        }
        let Some(adult) = c.level.get_entity(c.get(&nearest_adult).0) else { return false };
        if closer_than(&*c.body, adult, f64::from(follow_range_max_inclusive + 1)) && !closer_than(&*c.body, adult, f64::from(follow_range_min_inclusive)) {
            let target = WalkTarget::new(
                PositionTracker::Entity(EntityTracker::new_full(adult, target_eye, target_eye)),
                speed_modifier(&*c.body),
                follow_range_min_inclusive - 1,
            );
            let look = EntityTracker::new_full(adult, true, target_eye);
            c.set(&look_target, PositionTracker::Entity(look));
            c.set(&walk_target, target);
            true
        } else {
            false
        }
    })
}
