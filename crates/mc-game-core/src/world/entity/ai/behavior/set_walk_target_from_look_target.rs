//! `SetWalkTargetFromLookTarget`: worked example of a declarative behavior (see the module header of
//! [`crate::world::entity::ai::behavior`]).

use super::OneShot;
use super::declarative::BehaviorBuilder;
use crate::world::entity::ai::memory::WalkTarget;
use crate::world::entity::ai::memory::memory_module_type::{LOOK_TARGET, WALK_TARGET};
use crate::world::entity::entity::Entity;

/// `create(speedModifier, closeEnoughDistance)`.
pub fn create(speed_modifier: f32, close_enough_distance: i32) -> OneShot {
    create_with(|_| true, move |_| speed_modifier, close_enough_distance)
}

/// `create(canSetWalkTargetPredicate, speedModifier, closeEnoughDistance)`.
pub fn create_with(
    can_set_walk_target_predicate: impl Fn(&dyn Entity) -> bool + 'static,
    speed_modifier: impl Fn(&dyn Entity) -> f32 + 'static,
    close_enough_distance: i32,
) -> OneShot {
    let mut i = BehaviorBuilder::instance();
    let walk_target = i.absent(&WALK_TARGET);
    let look_target = i.present(&LOOK_TARGET);
    i.create(move |c| {
        if !can_set_walk_target_predicate(c.body()) {
            false
        } else {
            let target = WalkTarget::new(
                c.get(&look_target).clone(),
                speed_modifier(c.body()),
                close_enough_distance,
            );
            c.set(&walk_target, target);
            true
        }
    })
}
