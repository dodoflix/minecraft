//! `NonTameRandomTargetGoal<T>`: `NearestAttackableTargetGoal` that only runs while the mob is not tame.

use mc_game_core::world::entity::ai::goal::target::{NearestAttackableTargetGoal, TargetType};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use mc_game_core::world::entity::ai::targeting::TargetSelector;
use mc_game_core::world::entity::mob::tamable;
use mc_game_core::world::entity::Entity;

pub struct NonTameRandomTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl NonTameRandomTargetGoal {
    /// `NonTameRandomTargetGoal(mob, targetType, mustSee, subselector)`.
    pub fn new(target_type: TargetType, must_see: bool, subselector: Option<TargetSelector>) -> Self {
        Self { inner: NearestAttackableTargetGoal::build(target_type, 10, must_see, false, subselector) }
    }
}

impl Goal for NonTameRandomTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !tamable::is_tame(mob) && self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        match self.inner.target.and_then(|t| level.get_entity(t)) {
            Some(target) => self.inner.target_conditions.test(level, Some(&*mob), target),
            None => false,
        }
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.inner.stop(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.inner.flags()
    }
}
