//! `OcelotAttackGoal` (cats and ocelots stalk, rush and bite their target).

use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::mob::{self, combat};
use mc_game_core::world::entity::mob::navigation;
use mc_game_core::world::entity::{Entity, EntityId};

pub struct OcelotAttackGoal {
    target: Option<EntityId>,
    attack_time: i32,
}

impl OcelotAttackGoal {
    pub fn new() -> Self {
        Self { target: None, attack_time: 0 }
    }
}

impl Goal for OcelotAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        match mob.get_target() {
            None => false,
            Some(best_target) => {
                self.target = Some(best_target);
                true
            }
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target) = self.target.and_then(|t| level.get_entity(t)) else { return false };
        if !target.is_alive() {
            return false;
        }
        if mob.base().distance_to_sqr_vec(target.position()) > 225.0 {
            return false;
        }
        !navigation::is_done(mob, level) || self.can_use(mob, level)
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.target = None;
        navigation::stop(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = self.target else { return };
        let Some(target) = level.get_entity(target_id) else { return };
        mob::look_at_entity_full(mob, target, 30.0, 30.0);
        let width = mob.base().get_bb_width();
        let melee_radius_sqr = f64::from(width * 2.0 * (width * 2.0));
        let target_pos = target.position();
        let target_block_pos = target.block_position();
        let dist_sqr = mob.base().distance_to_sqr_xyz(target_pos.x, target_pos.y, target_pos.z);
        let mut speed_modifier = 0.8;
        if dist_sqr > melee_radius_sqr && dist_sqr < 16.0 {
            speed_modifier = 1.33;
        } else if dist_sqr < 225.0 {
            speed_modifier = 0.6;
        }
        navigation::move_to_entity(mob, level, target_block_pos, speed_modifier);
        self.attack_time = (self.attack_time - 1).max(0);
        if !(dist_sqr > melee_radius_sqr) && self.attack_time <= 0 {
            self.attack_time = 20;
            combat::do_hurt_target(mob, level, target_id);
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
