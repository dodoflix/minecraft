//! `WaterAvoidingRandomStrollGoal`.

use mc_core::random::RandomSource;
use super::{Goal, GoalFlags, RandomStrollGoal};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::util::land_random_pos;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;

pub const PROBABILITY: f32 = 0.001;

pub struct WaterAvoidingRandomStrollGoal {
    pub base: RandomStrollGoal,
    pub probability: f32,
}

impl WaterAvoidingRandomStrollGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self::with_probability(speed_modifier, PROBABILITY)
    }

    pub fn with_probability(speed_modifier: f64, probability: f32) -> Self {
        Self { base: RandomStrollGoal::new(speed_modifier), probability }
    }

    /// `getPosition()`.
    pub fn get_position(probability: f32, mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        if mob.is_in_water() {
            let pos = land_random_pos::get_pos(mob, level, 15, 7);
            match pos {
                None => RandomStrollGoal::default_position(mob, level),
                some => some,
            }
        } else if mob.base_mut().random.next_float() >= probability {
            land_random_pos::get_pos(mob, level, 10, 7)
        } else {
            RandomStrollGoal::default_position(mob, level)
        }
    }
}

impl Goal for WaterAvoidingRandomStrollGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let probability = self.probability;
        self.base.can_use_with(mob, level, |m, l| Self::get_position(probability, m, l))
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.stop_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.base.flags()
    }
}
