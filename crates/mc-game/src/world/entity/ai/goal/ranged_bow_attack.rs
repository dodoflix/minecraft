//! `RangedBowAttackGoal` (skeletons and other `Monster & RangedAttackMob` bow users; the mob overrides
//! `Entity::perform_ranged_attack`).

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::living_entity::using_item;
use crate::world::entity::mob::{self, combat, navigation, tick};
use crate::world::entity::Entity;
use crate::world::item::bow_item;
use crate::world::item::projectile_weapon_item::get_weapon_holding_hand;
use mc_core::random::RandomSource;
use mc_data::Item;

pub struct RangedBowAttackGoal {
    speed_modifier: f64,
    attack_interval_min: i32,
    attack_radius_sqr: f32,
    attack_time: i32,
    see_time: i32,
    strafing_clockwise: bool,
    strafing_backwards: bool,
    strafing_time: i32,
}

impl RangedBowAttackGoal {
    /// `RangedBowAttackGoal(mob, speedModifier, attackIntervalMin, attackRadius)`.
    pub fn new(speed_modifier: f64, attack_interval_min: i32, attack_radius: f32) -> Self {
        Self {
            speed_modifier,
            attack_interval_min,
            attack_radius_sqr: attack_radius * attack_radius,
            attack_time: -1,
            see_time: 0,
            strafing_clockwise: false,
            strafing_backwards: false,
            strafing_time: -1,
        }
    }

    /// `setMinAttackInterval(ticks)`.
    pub fn set_min_attack_interval(&mut self, ticks: i32) {
        self.attack_interval_min = ticks;
    }

    /// `isHoldingBow()`.
    fn is_holding_bow(mob: &dyn Entity) -> bool {
        combat::is_holding(mob, Item::BOW)
    }
}

impl Goal for RangedBowAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        if tick::get_target_checked(mob, level).is_none() { false } else { Self::is_holding_bow(mob) }
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        (self.can_use(mob, level) || !navigation::is_done(mob, level)) && Self::is_holding_bow(mob)
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        mob.mob_mut().expect("mob").set_aggressive(true);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.mob_mut().expect("mob").set_aggressive(false);
        self.see_time = 0;
        self.attack_time = -1;
        using_item::stop_using_item(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = tick::get_target_checked(mob, level) else { return };
        let Some(target) = level.get_entity(target_id) else { return };
        let target_dist_sqr = mob.base().distance_to_sqr_xyz(target.base().x(), target.base().y(), target.base().z());
        let has_line_of_sight = mob.mob().expect("mob").sensing.has_line_of_sight(level, mob, target);
        let target_pos = target.block_position();
        let had_line_of_sight = self.see_time > 0;
        if has_line_of_sight != had_line_of_sight {
            self.see_time = 0;
        }
        if has_line_of_sight {
            self.see_time += 1;
        } else {
            self.see_time -= 1;
        }
        if !(target_dist_sqr > f64::from(self.attack_radius_sqr)) && self.see_time >= 20 {
            navigation::stop(mob, level);
            self.strafing_time += 1;
        } else {
            navigation::move_to_entity(mob, level, target_pos, self.speed_modifier);
            self.strafing_time = -1;
        }
        if self.strafing_time >= 20 {
            if f64::from(mob.base_mut().random.next_float()) < 0.3 {
                self.strafing_clockwise = !self.strafing_clockwise;
            }
            if f64::from(mob.base_mut().random.next_float()) < 0.3 {
                self.strafing_backwards = !self.strafing_backwards;
            }
            self.strafing_time = 0;
        }
        if self.strafing_time > -1 {
            if target_dist_sqr > f64::from(self.attack_radius_sqr * 0.75) {
                self.strafing_backwards = false;
            } else if target_dist_sqr < f64::from(self.attack_radius_sqr * 0.25) {
                self.strafing_backwards = true;
            }
            mob.mob_mut().expect("mob").move_control_mut().strafe(if self.strafing_backwards { -0.5 } else { 0.5 }, if self.strafing_clockwise { 0.5 } else { -0.5 });
            if let Some(vehicle) = navigation::controlled_vehicle(mob, level).filter(|v| level.get_entity(*v).is_some_and(|e| e.mob().is_some())) {
                level.with_entity(vehicle, |level, v| {
                    if let Some(target) = level.get_entity(target_id) {
                        combat::look_at(v, target, 30.0, 30.0);
                    }
                });
            }
            if let Some(target) = level.get_entity(target_id) {
                combat::look_at(mob, target, 30.0, 30.0);
            }
        } else if let Some(target) = level.get_entity(target_id) {
            mob::look_at_entity_full(mob, target, 30.0, 30.0);
        }
        if using_item::is_using_item(mob) {
            if !has_line_of_sight && self.see_time < -60 {
                using_item::stop_using_item(mob, level);
            } else if has_line_of_sight {
                let pull_time = using_item::get_ticks_using_item(mob);
                if pull_time >= 20 {
                    using_item::stop_using_item(mob, level);
                    mob.perform_ranged_attack(level, target_id, bow_item::get_power_for_time(pull_time));
                    self.attack_time = self.attack_interval_min;
                }
            }
        } else {
            self.attack_time -= 1;
            if self.attack_time <= 0 && self.see_time >= -60 {
                let hand = get_weapon_holding_hand(mob, Item::BOW);
                using_item::start_using_item(mob, level, hand);
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
