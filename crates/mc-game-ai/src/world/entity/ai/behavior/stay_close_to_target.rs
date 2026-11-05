//! `StayCloseToTarget`.

use mc_game_core::world::entity::ai::behavior::declarative::BehaviorBuilder;
use mc_game_core::world::entity::ai::behavior::position_tracker::PositionTracker;
use mc_game_core::world::entity::ai::behavior::OneShot;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::memory::memory_module_type::{LOOK_TARGET, WALK_TARGET};
use mc_game_core::world::entity::ai::memory::WalkTarget;
use mc_game_core::world::entity::entity::Entity;

/// `create(targetPositionGetter, shouldRunPredicate, closeEnough, tooFar, speedModifier)`.
pub fn create(
    target_position_getter: impl Fn(&ServerLevel, &dyn Entity) -> Option<PositionTracker> + 'static,
    should_run_predicate: impl Fn(&dyn Entity) -> bool + 'static,
    close_enough: i32,
    too_far: i32,
    speed_modifier: f32,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let look_target = i.registered(&LOOK_TARGET);
    let walk_target = i.registered(&WALK_TARGET);
    i.create(move |c| {
        let Some(target) = target_position_getter(&*c.level, &*c.body) else { return false };
        if !should_run_predicate(&*c.body) {
            return false;
        }
        if c.body.position().closer_than(target.current_position(&*c.level), f64::from(too_far)) {
            false
        } else {
            c.set(&look_target, target.clone());
            c.set(&walk_target, WalkTarget::new(target, speed_modifier, close_enough));
            true
        }
    })
}
