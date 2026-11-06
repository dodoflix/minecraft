//! `LeapAtTargetGoal`.

use mc_core::phys::Vec3;
use mc_core::random::RandomSource;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use mc_game_core::world::entity::{Entity, EntityId};

pub struct LeapAtTargetGoal {
    target: Option<EntityId>,
    yd: f32,
}

impl LeapAtTargetGoal {
    pub fn new(yd: f32) -> Self {
        Self { target: None, yd }
    }
}

impl Goal for LeapAtTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if mob.has_controlling_passenger() {
            return false;
        }
        self.target = mob.get_target();
        let Some(target) = self.target.and_then(|t| level.get_entity(t)) else {
            return false;
        };
        let d = mob.base().distance_to_sqr_vec(target.position());
        if !(4.0..=16.0).contains(&d) {
            return false;
        }
        if !mob.base().on_ground() {
            return false;
        }
        mob.base_mut()
            .random
            .next_int_bounded(reduced_tick_delay(5))
            == 0
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        !mob.base().on_ground()
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target) = self.target.and_then(|t| level.get_entity(t)) else {
            return;
        };
        let movement = mob.base().delta_movement();
        let mut delta = Vec3::new(
            target.base().x() - mob.base().x(),
            0.0,
            target.base().z() - mob.base().z(),
        );
        if delta.length_sqr() > 1.0E-7 {
            delta = delta.normalize().scale(0.4).add(movement.scale(0.2));
        }
        mob.base_mut()
            .set_delta_movement(Vec3::new(delta.x, f64::from(self.yd), delta.z));
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}
