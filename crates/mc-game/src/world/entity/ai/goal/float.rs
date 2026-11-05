//! `FloatGoal`.

use mc_core::random::RandomSource;
use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::fluid_interaction::{TAG_ENTITY_FLOATABLE};
use crate::world::entity::mob::{jump_control_jump, navigation};
use crate::world::entity::Entity;

pub struct FloatGoal {
    fluid: &'static str,
}

impl FloatGoal {
    /// `FloatGoal(mob)`: `mob.getNavigation().setCanFloat(true)`.
    pub fn new(mob: &mut dyn Entity) -> Self {
        Self::with_fluid(mob, TAG_ENTITY_FLOATABLE)
    }

    /// `FloatGoal(mob, fluid)`.
    pub fn with_fluid(mob: &mut dyn Entity, fluid: &'static str) -> Self {
        navigation::set_can_float(mob, true);
        Self { fluid }
    }
}

impl Goal for FloatGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        mob.get_fluid_height(self.fluid) > mob.get_fluid_jump_threshold() || mob.is_in_lava()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        if mob.base_mut().random.next_float() < 0.8 {
            jump_control_jump(mob);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump])
    }
}
