//! `MoveTowardsTargetGoal`.

use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::world::entity::ai::util::default_random_pos;
use mc_game_core::world::entity::mob::{navigation, tick};
use mc_game_core::world::entity::{Entity, EntityId};

pub struct MoveTowardsTargetGoal {
    target: Option<EntityId>,
    wanted_x: f64,
    wanted_y: f64,
    wanted_z: f64,
    speed_modifier: f64,
    within: f32,
}

impl MoveTowardsTargetGoal {
    /// `MoveTowardsTargetGoal(mob, speedModifier, within)`.
    pub fn new(speed_modifier: f64, within: f32) -> Self {
        Self {
            target: None,
            wanted_x: 0.0,
            wanted_y: 0.0,
            wanted_z: 0.0,
            speed_modifier,
            within,
        }
    }

    fn within_sqr(&self) -> f32 {
        self.within * self.within
    }
}

impl Goal for MoveTowardsTargetGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.target = tick::get_target_checked(mob, level);
        let Some(target) = self.target.and_then(|t| level.get_entity(t)) else {
            return false;
        };
        if mob.base().distance_to_sqr_vec(target.position()) > f64::from(self.within_sqr()) {
            return false;
        }
        let target_pos = target.position();
        match default_random_pos::get_pos_towards(
            mob,
            level,
            16,
            7,
            target_pos,
            f64::from((std::f64::consts::PI / 2.0) as f32),
        ) {
            None => false,
            Some(pos) => {
                self.wanted_x = pos.x;
                self.wanted_y = pos.y;
                self.wanted_z = pos.z;
                true
            }
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        !navigation::is_done(mob, level)
            && self
                .target
                .and_then(|t| level.get_entity(t))
                .is_some_and(|t| {
                    t.is_alive()
                        && mob.base().distance_to_sqr_vec(t.position())
                            < f64::from(self.within_sqr())
                })
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.target = None;
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to(
            mob,
            level,
            self.wanted_x,
            self.wanted_y,
            self.wanted_z,
            self.speed_modifier,
        );
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move])
    }
}
