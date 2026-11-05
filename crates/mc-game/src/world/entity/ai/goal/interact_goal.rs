//! `InteractGoal` (`LookAtPlayerGoal` that also holds the `MOVE` flag).

use super::look_at_player::{LookAtPlayerGoal, LookAtType};
use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;

pub struct InteractGoal {
    pub base: LookAtPlayerGoal,
}

impl InteractGoal {
    /// `InteractGoal(mob, lookAtType, lookDistance)`.
    pub fn new(mob: &dyn Entity, look_at_type: LookAtType, look_distance: f32) -> Self {
        let mut base = LookAtPlayerGoal::new(mob, look_at_type, look_distance);
        base.set_flags(GoalFlags::of(&[Flag::Look, Flag::Move]));
        Self { base }
    }

    /// `InteractGoal(mob, lookAtType, lookDistance, probability)`.
    pub fn with_probability(mob: &dyn Entity, look_at_type: LookAtType, look_distance: f32, probability: f32) -> Self {
        let mut base = LookAtPlayerGoal::with_probability(mob, look_at_type, look_distance, probability);
        base.set_flags(GoalFlags::of(&[Flag::Look, Flag::Move]));
        Self { base }
    }
}

impl Goal for InteractGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_use(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_continue_to_use(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.start(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.stop(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.tick(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.base.flags()
    }
}
