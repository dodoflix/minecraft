//! `ZombieAttackGoal` (extends `MeleeAttackGoal`: raises the arms shortly before the attack).

use super::{Goal, GoalFlags, MeleeAttackGoal};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;

pub struct ZombieAttackGoal {
    pub melee: MeleeAttackGoal,
    raise_arm_ticks: i32,
}

impl ZombieAttackGoal {
    /// `ZombieAttackGoal(zombie, speedModifier, trackTarget)`.
    pub fn new(speed_modifier: f64, track_target: bool) -> Self {
        Self { melee: MeleeAttackGoal::new(speed_modifier, track_target), raise_arm_ticks: 0 }
    }
}

impl Goal for ZombieAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.melee.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.start_impl(mob, level);
        self.raise_arm_ticks = 0;
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.stop_impl(mob, level);
        mob.mob_mut().expect("mob").set_aggressive(false);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.melee.tick_impl(mob, level);
        self.raise_arm_ticks += 1;
        let aggressive = self.raise_arm_ticks >= 5 && self.melee.get_ticks_until_next_attack() < self.melee.get_attack_interval() / 2;
        mob.mob_mut().expect("mob").set_aggressive(aggressive);
    }

    fn flags(&self) -> GoalFlags {
        self.melee.flags()
    }
}
