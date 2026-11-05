//! `MoveTowardsRestrictionGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::ai::util::default_random_pos;
use crate::world::entity::mob::navigation;
use crate::world::entity::Entity;
use mc_core::phys::Vec3;

pub struct MoveTowardsRestrictionGoal {
    wanted_x: f64,
    wanted_y: f64,
    wanted_z: f64,
    speed_modifier: f64,
}

impl MoveTowardsRestrictionGoal {
    pub fn new(move_speed_modifier: f64) -> Self {
        Self { wanted_x: 0.0, wanted_y: 0.0, wanted_z: 0.0, speed_modifier: move_speed_modifier }
    }
}

impl Goal for MoveTowardsRestrictionGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let pos = mob.block_position();
        let Some(home) = mob.mob().map(|m| m.home_position) else { return false };
        if mob.mob().is_some_and(|m| m.is_within_home_pos(pos)) {
            return false;
        }
        // `(float) (Math.PI / 2)` widened to the double parameter
        let Some(pos) = default_random_pos::get_pos_towards(mob, level, 16, 7, Vec3::at_bottom_center_of(&home), f64::from((std::f64::consts::PI / 2.0) as f32)) else {
            return false;
        };
        self.wanted_x = pos.x;
        self.wanted_y = pos.y;
        self.wanted_z = pos.z;
        true
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !navigation::is_done(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to(mob, level, self.wanted_x, self.wanted_y, self.wanted_z, self.speed_modifier);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
