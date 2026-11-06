//! `RandomLookAroundGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use mc_core::random::RandomSource;

pub struct RandomLookAroundGoal {
    rel_x: f64,
    rel_z: f64,
    look_time: i32,
}

impl RandomLookAroundGoal {
    pub fn new() -> Self {
        Self {
            rel_x: 0.0,
            rel_z: 0.0,
            look_time: 0,
        }
    }
}

impl Default for RandomLookAroundGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for RandomLookAroundGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        mob.base_mut().random.next_float() < 0.02
    }

    fn can_continue_to_use(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        self.look_time >= 0
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        let rnd = (std::f64::consts::PI * 2.0) * mob.base_mut().random.next_double();
        self.rel_x = rnd.cos();
        self.rel_z = rnd.sin();
        self.look_time = 20 + mob.base_mut().random.next_int_bounded(20);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.look_time -= 1;
        let (x, eye_y, z) = (
            mob.base().x() + self.rel_x,
            mob.base().get_eye_y(),
            mob.base().z() + self.rel_z,
        );
        crate::world::entity::mob::look_at_xyz(mob, x, eye_y, z);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
