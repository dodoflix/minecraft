//! `NearestAttackableWitchTargetGoal` (extends `NearestAttackableTargetGoal`; `Raider` witches stop targeting while drinking).

use super::NearestAttackableTargetGoal;
use crate::server::level::ServerLevel;
use crate::world::entity::ai::goal::{Goal, GoalFlags};
use crate::world::entity::Entity;

pub struct NearestAttackableWitchTargetGoal {
    pub inner: NearestAttackableTargetGoal,
    can_attack: bool,
}

impl NearestAttackableWitchTargetGoal {
    /// `NearestAttackableWitchTargetGoal(raider, targetType, randomInterval, mustSee, mustReach, subselector)`: build the
    /// `NearestAttackableTargetGoal` with [`NearestAttackableTargetGoal::build`].
    pub fn new(inner: NearestAttackableTargetGoal) -> Self {
        Self { inner, can_attack: true }
    }

    /// `setCanAttack(canAttack)`.
    pub fn set_can_attack(&mut self, can_attack: bool) {
        self.can_attack = can_attack;
    }
}

impl Goal for NearestAttackableWitchTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_attack && self.inner.can_use_impl(mob, level)
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
