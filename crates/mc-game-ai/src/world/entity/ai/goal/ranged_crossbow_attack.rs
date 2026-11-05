//! `RangedCrossbowAttackGoal` (pillagers, piglins: the mob overrides `Entity::perform_ranged_attack` and the
//! `CrossbowAttackMob` hooks `set_charging_crossbow` / `on_crossbow_attack_performed`).

use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::living_entity::using_item;
use mc_game_core::world::entity::mob::{self, combat, navigation, tick};
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::crossbow_item;
use mc_game_core::world::item::projectile_weapon_item::get_weapon_holding_hand;
use mc_core::random::RandomSource;
use mc_data::Item;

/// `PATHFINDING_DELAY_RANGE = TimeUtil.rangeOfSeconds(1, 2)`.
const PATHFINDING_DELAY_MIN: i32 = 20;
const PATHFINDING_DELAY_MAX: i32 = 40;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CrossbowState {
    Uncharged,
    Charging,
    Charged,
    ReadyToAttack,
}

pub struct RangedCrossbowAttackGoal {
    crossbow_state: CrossbowState,
    speed_modifier: f64,
    attack_radius_sqr: f32,
    see_time: i32,
    attack_delay: i32,
    update_path_delay: i32,
}

impl RangedCrossbowAttackGoal {
    /// `RangedCrossbowAttackGoal(mob, speedModifier, attackRadius)`.
    pub fn new(speed_modifier: f64, attack_radius: f32) -> Self {
        Self { crossbow_state: CrossbowState::Uncharged, speed_modifier, attack_radius_sqr: attack_radius * attack_radius, see_time: 0, attack_delay: 0, update_path_delay: 0 }
    }

    fn is_valid_target(mob: &dyn Entity, level: &ServerLevel) -> bool {
        tick::get_target_checked(mob, level).and_then(|t| level.get_entity(t)).is_some_and(|t| t.is_alive())
    }

    fn is_holding_crossbow(mob: &dyn Entity) -> bool {
        combat::is_holding(mob, Item::CROSSBOW)
    }

    fn can_run(&self) -> bool {
        self.crossbow_state == CrossbowState::Uncharged
    }
}

impl Goal for RangedCrossbowAttackGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::is_valid_target(mob, level) && Self::is_holding_crossbow(mob)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::is_valid_target(mob, level) && (self.can_use(mob, level) || !navigation::is_done(mob, level)) && Self::is_holding_crossbow(mob)
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        mob.mob_mut().expect("mob").set_aggressive(false);
        mob.set_target(level, None);
        self.see_time = 0;
        if using_item::is_using_item(mob) {
            using_item::stop_using_item(mob, level);
            mob.set_charging_crossbow(false);
            // `getUseItem().set(CHARGED_PROJECTILES, EMPTY)`: after `stopUsingItem()` that is the shared `ItemStack.EMPTY`, the
            // crossbow in the hand is untouched
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(target_id) = tick::get_target_checked(mob, level) else { return };
        let Some(target) = level.get_entity(target_id) else { return };
        let has_line_of_sight = mob.mob().expect("mob").sensing.has_line_of_sight(level, mob, target);
        let had_line_of_sight = self.see_time > 0;
        if has_line_of_sight != had_line_of_sight {
            self.see_time = 0;
        }
        if has_line_of_sight {
            self.see_time += 1;
        } else {
            self.see_time -= 1;
        }
        let distance_to_sqr = mob.base().distance_to_sqr_vec(target.position());
        let target_pos = target.block_position();
        let needs_to_move = (distance_to_sqr > f64::from(self.attack_radius_sqr) || self.see_time < 5) && self.attack_delay == 0;
        if needs_to_move {
            self.update_path_delay -= 1;
            if self.update_path_delay <= 0 {
                let speed = if self.can_run() { self.speed_modifier } else { self.speed_modifier * 0.5 };
                navigation::move_to_entity(mob, level, target_pos, speed);
                self.update_path_delay = mob.base_mut().random.next_int_between_inclusive(PATHFINDING_DELAY_MIN, PATHFINDING_DELAY_MAX);
            }
        } else {
            self.update_path_delay = 0;
            navigation::stop(mob, level);
        }
        if let Some(target) = level.get_entity(target_id) {
            mob::look_at_entity_full(mob, target, 30.0, 30.0);
        }
        match self.crossbow_state {
            CrossbowState::Uncharged => {
                if !needs_to_move {
                    let hand = get_weapon_holding_hand(mob, Item::CROSSBOW);
                    using_item::start_using_item(mob, level, hand);
                    self.crossbow_state = CrossbowState::Charging;
                    mob.set_charging_crossbow(true);
                }
            }
            CrossbowState::Charging => {
                if !using_item::is_using_item(mob) {
                    self.crossbow_state = CrossbowState::Uncharged;
                }
                let pull_time = using_item::get_ticks_using_item(mob);
                let use_item = using_item::get_use_item(mob);
                if pull_time >= crossbow_item::get_charge_duration(level, &use_item, mob) {
                    using_item::release_using_item(mob, level);
                    self.crossbow_state = CrossbowState::Charged;
                    self.attack_delay = 20 + mob.base_mut().random.next_int_bounded(20);
                    mob.set_charging_crossbow(false);
                }
            }
            CrossbowState::Charged => {
                self.attack_delay -= 1;
                if self.attack_delay == 0 {
                    self.crossbow_state = CrossbowState::ReadyToAttack;
                }
            }
            CrossbowState::ReadyToAttack => {
                if has_line_of_sight {
                    mob.perform_ranged_attack(level, target_id, 1.0);
                    self.crossbow_state = CrossbowState::Uncharged;
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
