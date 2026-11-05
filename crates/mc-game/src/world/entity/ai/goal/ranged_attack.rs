//! `RangedAttackGoal`.

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::mob::{self, navigation, tick};
use crate::world::entity::{Entity, EntityId};
use mc_core::math::mth;

pub struct RangedAttackGoal {
    target: Option<EntityId>,
    attack_time: i32,
    speed_modifier: f64,
    see_time: i32,
    attack_interval_min: i32,
    attack_interval_max: i32,
    attack_radius: f32,
    attack_radius_sqr: f32,
}

impl RangedAttackGoal {
    /// `RangedAttackGoal(mob, speedModifier, attackInterval, attackRadius)`.
    pub fn new(speed_modifier: f64, attack_interval: i32, attack_radius: f32) -> Self {
        Self::with_interval_range(speed_modifier, attack_interval, attack_interval, attack_radius)
    }

    /// `RangedAttackGoal(mob, speedModifier, attackIntervalMin, attackIntervalMax, attackRadius)`.. The mob must override
    /// `Entity::perform_ranged_attack` (vanilla's constructor requires `RangedAttackMob`).
    pub fn with_interval_range(speed_modifier: f64, attack_interval_min: i32, attack_interval_max: i32, attack_radius: f32) -> Self {
        Self {
            target: None,
            attack_time: -1,
            speed_modifier,
            see_time: 0,
            attack_interval_min,
            attack_interval_max,
            attack_radius,
            attack_radius_sqr: attack_radius * attack_radius,
        }
    }
}

impl Goal for RangedAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        match tick::get_target_checked(mob, level) {
            Some(best) if level.get_entity(best).is_some_and(|t| t.is_alive()) => {
                self.target = Some(best);
                true
            }
            _ => false,
        }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use(mob, level) || self.target.and_then(|t| level.get_entity(t)).is_some_and(|t| t.is_alive()) && !navigation::is_done(mob, level)
    }

    fn stop(&mut self, _mob: &mut dyn Entity, _level: &mut ServerLevel) {
        self.target = None;
        self.see_time = 0;
        self.attack_time = -1;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = self.target else { return };
        let Some(target) = level.get_entity(target_id) else { return };
        let target_dist_sqr = mob.base().distance_to_sqr_xyz(target.base().x(), target.base().y(), target.base().z());
        let has_line_of_sight = mob.mob().expect("mob").sensing.has_line_of_sight(level, mob, target);
        if has_line_of_sight {
            self.see_time += 1;
        } else {
            self.see_time = 0;
        }
        let target_pos = target.block_position();
        if !(target_dist_sqr > f64::from(self.attack_radius_sqr)) && self.see_time >= 5 {
            navigation::stop(mob, level);
        } else {
            navigation::move_to_entity(mob, level, target_pos, self.speed_modifier);
        }
        if let Some(target) = level.get_entity(target_id) {
            mob::look_at_entity_full(mob, target, 30.0, 30.0);
        }
        self.attack_time -= 1;
        if self.attack_time == 0 {
            if !has_line_of_sight {
                return;
            }
            let dist = (target_dist_sqr.sqrt() as f32) / self.attack_radius;
            let power = mth::clamp_f32(dist, 0.1, 1.0);
            mob.perform_ranged_attack(level, target_id, power);
            self.attack_time = mth::floor_f32(dist * (self.attack_interval_max - self.attack_interval_min) as f32 + self.attack_interval_min as f32);
        } else if self.attack_time < 0 {
            self.attack_time = mth::floor_f64(mth::lerp_f64(target_dist_sqr.sqrt() / f64::from(self.attack_radius), f64::from(self.attack_interval_min), f64::from(self.attack_interval_max)));
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
