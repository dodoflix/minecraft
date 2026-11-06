//! `SitWhenOrderedToGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::mob::{combat, navigation, tamable};

pub struct SitWhenOrderedToGoal;

impl SitWhenOrderedToGoal {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SitWhenOrderedToGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for SitWhenOrderedToGoal {
    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) -> bool {
        tamable::is_ordered_to_sit(mob)
    }

    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let ordered_to_sit = tamable::is_ordered_to_sit(mob);
        if !ordered_to_sit && !tamable::is_tame(mob) {
            return false;
        }
        if mob.is_in_water() {
            return false;
        }
        if !mob.base().on_ground() {
            return false;
        }
        let Some(owner) = tamable::get_owner(mob, level).and_then(|o| level.get_entity(o)) else {
            return true;
        };
        if mob.base().distance_to_sqr_vec(owner.position()) < 144.0
            && combat::get_last_hurt_by_mob(owner, level).is_some()
        {
            return false;
        }
        ordered_to_sit
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
        tamable::set_in_sitting_pose(mob, true);
    }

    fn stop(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        tamable::set_in_sitting_pose(mob, false);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Jump, Flag::Move])
    }
}
