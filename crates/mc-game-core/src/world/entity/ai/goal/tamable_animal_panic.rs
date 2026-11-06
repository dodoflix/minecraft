//! `TamableAnimal.TamableAnimalPanicGoal`.

use super::{Goal, GoalFlags, PanicGoal};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob::tamable;

pub struct TamableAnimalPanicGoal {
    panic: PanicGoal,
}

impl TamableAnimalPanicGoal {
    /// `TamableAnimalPanicGoal(speedModifier)`.
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            panic: PanicGoal::new(speed_modifier),
        }
    }

    /// `TamableAnimalPanicGoal(speedModifier, panicCausingDamageTypes)`.
    pub fn with_tag(speed_modifier: f64, panic_causing_damage_types: &'static str) -> Self {
        Self {
            panic: PanicGoal::with_tag(speed_modifier, panic_causing_damage_types),
        }
    }
}

impl Goal for TamableAnimalPanicGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.panic.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.panic.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.panic.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.panic.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        if !tamable::unable_to_move_to_owner(mob, level)
            && tamable::should_try_teleport_to_owner(mob, level)
        {
            tamable::try_to_teleport_to_owner(mob, level);
        }
        self.panic.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.panic.flags()
    }

    fn as_panic_goal(&self) -> Option<&PanicGoal> {
        Some(&self.panic)
    }
}
