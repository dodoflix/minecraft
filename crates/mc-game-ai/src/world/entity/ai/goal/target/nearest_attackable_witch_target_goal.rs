//! `NearestAttackableWitchTargetGoal` (extends `NearestAttackableTargetGoal`; a witch stops targeting players while its healing goal is
//! cooling down).

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::target::NearestAttackableTargetGoal;
use mc_game_core::world::entity::ai::goal::{Goal, GoalFlags};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct NearestAttackableWitchTargetGoal {
    pub inner: NearestAttackableTargetGoal,
    /// `canAttack`, shared with the witch (`setCanAttack`).
    can_attack: Arc<AtomicBool>,
}

impl NearestAttackableWitchTargetGoal {
    /// `NearestAttackableWitchTargetGoal(raider, targetType, randomInterval, mustSee, mustReach, subselector)`: build the
    /// `NearestAttackableTargetGoal` with [`NearestAttackableTargetGoal::build`]; the witch keeps `can_attack` to call
    /// `setCanAttack` from its `aiStep`.
    pub fn new(inner: NearestAttackableTargetGoal, can_attack: Arc<AtomicBool>) -> Self {
        Self { inner, can_attack }
    }
}

impl Goal for NearestAttackableWitchTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_attack.load(Ordering::Relaxed) && self.inner.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.inner.can_continue_to_use(mob, level)
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
