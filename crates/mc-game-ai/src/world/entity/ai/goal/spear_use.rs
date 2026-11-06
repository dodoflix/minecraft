//! `SpearUseGoal` (monsters wielding a kinetic weapon charge, then retreat).

use mc_core::phys::Vec3;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::entity::ai::goal::{Flag, Goal, GoalFlags, reduced_tick_delay};
use mc_game_core::world::entity::ai::util::land_random_pos;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::living_entity::using_item;
use mc_game_core::world::entity::mob::{self, combat, navigation, tick};
use mc_game_core::world::item::InteractionHand;
use mc_protocol::components::KINETIC_WEAPON;

/// `MAX_FLEEING_TIME = reducedTickDelay(100)` (a `double`).
fn max_fleeing_time() -> f64 {
    f64::from(reduced_tick_delay(100))
}

/// `SpearUseGoal.SpearUseState`.
struct SpearUseState {
    engage_time: i32,
    fleeing_time: i32,
    away_pos: Option<Vec3>,
    done: bool,
}

impl SpearUseState {
    fn new() -> Self {
        Self {
            engage_time: -1,
            fleeing_time: -1,
            away_pos: None,
            done: false,
        }
    }

    fn not_engaged_yet(&self) -> bool {
        self.engage_time < 0
    }

    fn start_engagement(&mut self, spear_down_time: i32) {
        self.engage_time = spear_down_time;
    }

    fn tick_and_check_engagement(&mut self) -> bool {
        if self.engage_time > 0 {
            self.engage_time -= 1;
            if self.engage_time == 0 {
                return true;
            }
        }
        false
    }

    fn tick_and_check_fleeing(&mut self) -> bool {
        if self.fleeing_time > 0 {
            self.fleeing_time += 1;
            if f64::from(self.fleeing_time) > max_fleeing_time() {
                self.done = true;
                return true;
            }
        }
        false
    }
}

pub struct SpearUseGoal {
    state: Option<SpearUseState>,
    speed_modifier_when_charging: f64,
    speed_modifier_when_repositioning: f64,
    approach_distance_sq: f32,
    target_in_range_radius_sq: f32,
}

impl SpearUseGoal {
    /// `SpearUseGoal(mob, speedModifierWhenCharging, speedModifierWhenRepositioning, approachDistance, targetInRangeRadius)`.
    pub fn new(
        speed_modifier_when_charging: f64,
        speed_modifier_when_repositioning: f64,
        approach_distance: f32,
        target_in_range_radius: f32,
    ) -> Self {
        Self {
            state: None,
            speed_modifier_when_charging,
            speed_modifier_when_repositioning,
            approach_distance_sq: approach_distance * approach_distance,
            target_in_range_radius_sq: target_in_range_radius * target_in_range_radius,
        }
    }

    fn able_to_attack(mob: &dyn Entity, level: &ServerLevel) -> bool {
        tick::get_target_checked(mob, level).is_some()
            && mob
                .get_item_by_slot(EquipmentSlot::MainHand)
                .get(&KINETIC_WEAPON)
                .is_some()
    }

    /// `getKineticWeaponUseDuration()`.
    fn get_kinetic_weapon_use_duration(mob: &dyn Entity) -> i32 {
        let duration_ticks = mob
            .get_item_by_slot(EquipmentSlot::MainHand)
            .get(&KINETIC_WEAPON)
            .map_or(0, |k| {
                k.delay_ticks.wrapping_add(
                    k.damage_conditions
                        .as_ref()
                        .map_or(0, |c| c.max_duration_ticks),
                )
            });
        reduced_tick_delay(duration_ticks)
    }
}

impl Goal for SpearUseGoal {
    fn can_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        Self::able_to_attack(mob, level) && !using_item::is_using_item(mob)
    }

    fn can_continue_to_use(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) -> bool {
        self.state.as_ref().is_some_and(|s| !s.done) && Self::able_to_attack(mob, level)
    }

    fn start(&mut self, mob: &mut dyn Entity, _level: &mut ServerLevel) {
        mob.mob_mut().expect("mob").set_aggressive(true);
        self.state = Some(SpearUseState::new());
    }

    fn stop(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        navigation::stop(mob, level);
        mob.mob_mut().expect("mob").set_aggressive(false);
        self.state = None;
        using_item::stop_using_item(mob, level);
    }

    fn tick(&mut self, mob: &mut dyn Entity, level: &mut ServerLevel) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let Some(target_id) = tick::get_target_checked(mob, level) else {
            return;
        };
        let Some(target) = level.get_entity(target_id) else {
            return;
        };
        let (target_pos_vec, target_block_pos) = (target.position(), target.block_position());
        let target_dist_sqr =
            mob.base()
                .distance_to_sqr_xyz(target.base().x(), target.base().y(), target.base().z());
        let speed_modifier = combat::root_charge_speed_modifier(mob, level);
        let mount_distance = if mob.base().is_passenger() { 2 } else { 0 };
        combat::look_at(mob, target, 30.0, 30.0);
        mob::look_at_entity_full(mob, target, 30.0, 30.0);
        if state.not_engaged_yet() {
            if target_dist_sqr > f64::from(self.approach_distance_sq) {
                navigation::move_to_entity(
                    mob,
                    level,
                    target_block_pos,
                    f64::from(speed_modifier) * self.speed_modifier_when_repositioning,
                );
                return;
            }
            state.start_engagement(Self::get_kinetic_weapon_use_duration(mob));
            using_item::start_using_item(mob, level, InteractionHand::MainHand);
        }
        if state.tick_and_check_engagement() {
            using_item::stop_using_item(mob, level);
            let distance = target_dist_sqr.sqrt();
            state.away_pos = land_random_pos::get_pos_away_range(
                mob,
                level,
                (f64::from(9 + mount_distance) - distance).max(0.0),
                (f64::from(11 + mount_distance) - distance).max(1.0),
                7,
                target_pos_vec,
            );
            state.fleeing_time = 1;
        }
        if !state.tick_and_check_fleeing() {
            if let Some(away) = state.away_pos {
                navigation::move_to(
                    mob,
                    level,
                    away.x,
                    away.y,
                    away.z,
                    f64::from(speed_modifier) * self.speed_modifier_when_repositioning,
                );
                if navigation::is_done(mob, level) {
                    if state.fleeing_time > 0 {
                        state.done = true;
                        return;
                    }
                    state.away_pos = None;
                }
            } else {
                navigation::move_to_entity(
                    mob,
                    level,
                    target_block_pos,
                    f64::from(speed_modifier) * self.speed_modifier_when_charging,
                );
                if target_dist_sqr < f64::from(self.target_in_range_radius_sq)
                    || navigation::is_done(mob, level)
                {
                    let distance = target_dist_sqr.sqrt();
                    state.away_pos = land_random_pos::get_pos_away_range(
                        mob,
                        level,
                        f64::from(6 + mount_distance) - distance,
                        f64::from(7 + mount_distance) - distance,
                        7,
                        target_pos_vec,
                    );
                }
            }
        }
    }

    fn flags(&self) -> GoalFlags {
        GoalFlags::of(&[Flag::Move, Flag::Look])
    }
}
