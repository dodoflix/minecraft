//! `MeleeAttackGoal` (`ZombieAttackGoal` and the mob-specific subclasses embed it and call its `protected` methods).

use super::{Flag, Goal, GoalFlags};
use crate::server::level::ServerLevel;
use crate::world::entity::mob::{self, combat, ext, navigation, tick};
use crate::world::entity::{Entity, EntityId};
use crate::world::item::InteractionHand;
use crate::world::level::pathfinder::Path;
use mc_core::random::RandomSource;

const COOLDOWN_BETWEEN_CAN_USE_CHECKS: i64 = 20;
const ATTACK_INTERVAL: i32 = 20;

pub struct MeleeAttackGoal {
    speed_modifier: f64,
    following_target_even_if_not_seen: bool,
    path: Option<Path>,
    pathed_target_x: f64,
    pathed_target_y: f64,
    pathed_target_z: f64,
    ticks_until_next_path_recalculation: i32,
    ticks_until_next_attack: i32,
    last_can_use_check: i64,
}

/// A replacement for `checkAndPerformAttack(target)` (subclass override); gets the goal back for its protected methods.
pub type AttackFn<'a> = &'a mut dyn FnMut(&mut MeleeAttackGoal, &mut dyn Entity, &mut ServerLevel, EntityId);

impl MeleeAttackGoal {
    /// `MeleeAttackGoal(mob, speedModifier, followingTargetEvenIfNotSeen)`.
    pub fn new(speed_modifier: f64, following_target_even_if_not_seen: bool) -> Self {
        Self {
            speed_modifier,
            following_target_even_if_not_seen,
            path: None,
            pathed_target_x: 0.0,
            pathed_target_y: 0.0,
            pathed_target_z: 0.0,
            ticks_until_next_path_recalculation: 0,
            ticks_until_next_attack: 0,
            last_can_use_check: 0,
        }
    }

    /// `canUse()`.
    pub fn can_use_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let time = level.game_time();
        if time - self.last_can_use_check < COOLDOWN_BETWEEN_CAN_USE_CHECKS {
            return false;
        }
        self.last_can_use_check = time;
        let Some(target_id) = tick::get_target_checked(mob, level) else { return false };
        let Some(target) = level.get_entity(target_id) else { return false };
        if !target.is_alive() {
            return false;
        }
        let target_pos = target.block_position();
        self.path = navigation::create_path_entity(mob, level, target_pos, 0);
        if self.path.is_some() {
            return true;
        }
        match level.get_entity(target_id) {
            Some(target) => combat::is_within_melee_attack_range(mob, level, target),
            None => false,
        }
    }

    /// `canContinueToUse()`.
    pub fn can_continue_to_use_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        let Some(target_id) = tick::get_target_checked(mob, level) else { return false };
        let Some(target) = level.get_entity(target_id) else { return false };
        if !target.is_alive() {
            return false;
        }
        if !self.following_target_even_if_not_seen {
            return !navigation::is_done(mob, level);
        }
        if !mob.mob().expect("mob").is_within_home_pos(target.block_position()) {
            return false;
        }
        !(target.is_player() && (target.is_spectator() || ext::is_creative_player(target)))
    }

    /// `start()`.
    pub fn start_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::move_to_path(mob, level, self.path.clone(), self.speed_modifier);
        mob.mob_mut().expect("mob").set_aggressive(true);
        self.ticks_until_next_path_recalculation = 0;
        self.ticks_until_next_attack = 0;
    }

    /// `stop()`.
    pub fn stop_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        // `!EntitySelector.NO_CREATIVE_OR_SPECTATOR.test(target)` on `getTarget()` (already filtered by `asValidTarget`)
        if let Some(target) = tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t)) {
            if target.is_player() && (target.is_spectator() || ext::is_creative_player(target)) {
                mob.set_target(level, None);
            }
        }
        mob.mob_mut().expect("mob").set_aggressive(false);
        navigation::stop(mob, level);
    }

    /// `tick()` with the vanilla `checkAndPerformAttack`.
    pub fn tick_impl(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tick_with(mob, level, &mut |goal, mob, level, target| goal.check_and_perform_attack(mob, level, target));
    }

    /// `tick()` with an overridden `checkAndPerformAttack`.
    pub fn tick_with(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, attack: AttackFn<'_>) {
        let Some(target_id) = tick::get_target_checked(mob, level) else { return };
        {
            let Some(target) = level.get_entity(target_id) else { return };
            mob::look_at_entity_full(mob, target, 30.0, 30.0);
        }
        self.ticks_until_next_path_recalculation = (self.ticks_until_next_path_recalculation - 1).max(0);
        let Some(target) = level.get_entity(target_id) else { return };
        if (self.following_target_even_if_not_seen || mob.mob().expect("mob").sensing.has_line_of_sight(level, mob, target))
            && self.ticks_until_next_path_recalculation <= 0
            && (self.pathed_target_x == 0.0 && self.pathed_target_y == 0.0 && self.pathed_target_z == 0.0
                || target.base().distance_to_sqr_xyz(self.pathed_target_x, self.pathed_target_y, self.pathed_target_z) >= 1.0
                || mob.base_mut().random.next_float() < 0.05)
        {
            let (tx, ty, tz) = (target.base().x(), target.base().y(), target.base().z());
            let target_pos = target.block_position();
            self.pathed_target_x = tx;
            self.pathed_target_y = ty;
            self.pathed_target_z = tz;
            self.ticks_until_next_path_recalculation = 4 + mob.base_mut().random.next_int_bounded(7);
            let target_distance_sqr = mob.base().distance_to_sqr_xyz(tx, ty, tz);
            if target_distance_sqr > 1024.0 {
                self.ticks_until_next_path_recalculation += 10;
            } else if target_distance_sqr > 256.0 {
                self.ticks_until_next_path_recalculation += 5;
            }
            if !navigation::move_to_entity_reach(mob, level, target_pos, 0, self.speed_modifier) {
                self.ticks_until_next_path_recalculation += 15;
            }
            self.ticks_until_next_path_recalculation = self.adjusted_tick_delay(self.ticks_until_next_path_recalculation);
        }
        self.ticks_until_next_attack = (self.ticks_until_next_attack - 1).max(0);
        attack(self, mob, level, target_id);
    }

    /// `checkAndPerformAttack(target)`.
    pub fn check_and_perform_attack(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel, target: EntityId) {
        if self.can_perform_attack(mob, level, target) {
            self.reset_attack_cooldown();
            combat::swing_for_attack(mob, level, InteractionHand::MainHand);
            combat::do_hurt_target(mob, level, target);
        }
    }

    /// `resetAttackCooldown()`.
    pub fn reset_attack_cooldown(&mut self) {
        self.ticks_until_next_attack = self.adjusted_tick_delay(ATTACK_INTERVAL);
    }

    /// `isTimeToAttack()`.
    pub fn is_time_to_attack(&self) -> bool {
        self.ticks_until_next_attack <= 0
    }

    /// `canPerformAttack(target)`.
    pub fn can_perform_attack(&self, mob: &dyn Entity, level: &ServerLevel, target: EntityId) -> bool {
        let Some(target) = level.get_entity(target) else { return false };
        self.is_time_to_attack() && combat::is_within_melee_attack_range(mob, level, target) && mob.mob().expect("mob").sensing.has_line_of_sight(level, mob, target)
    }

    /// `getTicksUntilNextAttack()`.
    pub fn get_ticks_until_next_attack(&self) -> i32 {
        self.ticks_until_next_attack
    }

    /// `getAttackInterval()`.
    pub fn get_attack_interval(&self) -> i32 {
        self.adjusted_tick_delay(ATTACK_INTERVAL)
    }
}

impl Goal for MeleeAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_use_impl(mob, level)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.can_continue_to_use_impl(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.start_impl(mob, level);
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.stop_impl(mob, level);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        self.tick_impl(mob, level);
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
