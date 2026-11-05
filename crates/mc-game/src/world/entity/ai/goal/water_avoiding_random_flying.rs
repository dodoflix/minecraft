//! `WaterAvoidingRandomFlyingGoal`.

use super::{Goal, GoalFlags, WaterAvoidingRandomStrollGoal};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::util::{air_and_water_random_pos, hover_random_pos};
use crate::world::entity::Entity;
use mc_core::phys::Vec3;

pub struct WaterAvoidingRandomFlyingGoal {
    pub base: WaterAvoidingRandomStrollGoal,
}

impl WaterAvoidingRandomFlyingGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self { base: WaterAvoidingRandomStrollGoal::new(speed_modifier) }
    }

    /// `getPosition()`.
    pub fn get_position(mob: &mut dyn Entity, level: &mut ServerLevel) -> Option<Vec3> {
        let wander_direction = mob.get_view_vector(0.0);
        let half_pi = (std::f64::consts::PI / 2.0) as f32;
        let ground_based = hover_random_pos::get_pos(mob, level, 8, 7, wander_direction.x, wander_direction.z, half_pi, 3, 1);
        match ground_based {
            Some(p) => Some(p),
            None => air_and_water_random_pos::get_pos(mob, level, 8, 4, -2, wander_direction.x, wander_direction.z, f64::from(half_pi)),
        }
    }
}

impl Goal for WaterAvoidingRandomFlyingGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.base.can_use_with(mob, level, Self::get_position)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.base.base.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.base.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.base.base.stop_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        self.base.flags()
    }
}
