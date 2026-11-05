//! `Swim`: jumps in a fluid with the given chance per tick.

use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::entity::Entity;
use crate::world::entity::fluid_interaction::TAG_ENTITY_FLOATABLE;
use crate::world::entity::mob::jump_control_jump;
use mc_core::random::RandomSource;

pub struct Swim {
    chance: f32,
    fluid: &'static str,
}

impl Swim {
    /// `new Swim(chance)`.
    pub fn new(chance: f32) -> BehaviorBase<Swim> {
        Self::with_fluid(chance, TAG_ENTITY_FLOATABLE)
    }

    /// `new Swim(chance, fluid)`.
    pub fn with_fluid(chance: f32, fluid: &'static str) -> BehaviorBase<Swim> {
        BehaviorBase::new(entry_condition![], Swim { chance, fluid })
    }
}

impl Behavior for Swim {
    fn check_extra_start_conditions(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity) -> bool {
        body.get_fluid_height(self.fluid) > body.base().get_fluid_jump_threshold() || body.is_in_lava()
    }

    fn can_still_use(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) -> bool {
        self.check_extra_start_conditions(level, body)
    }

    fn tick(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if body.base_mut().random.next_float() < self.chance {
            jump_control_jump(body);
        }
    }
}
