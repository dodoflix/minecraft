//! `SpearAttack`: charge at the target with a kinetic weapon, reposition when close.

use super::spear_approach::{
    able_to_attack, get_target, root_charge_speed_modifier, target_distance_sqr,
};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::SpearStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    LOOK_TARGET, SPEAR_CHARGE_POSITION, SPEAR_ENGAGE_TIME, SPEAR_STATUS,
};
use mc_game_core::world::entity::ai::util::land_random_pos;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::equipment::EquipmentSlot;
use mc_game_core::world::entity::living_entity::using_item;
use mc_game_core::world::entity::mob::navigation;
use mc_game_core::world::item::InteractionHand;
use mc_protocol::components::KINETIC_WEAPON;

/// `MIN_REPOSITION_DISTANCE` / `MAX_REPOSITION_DISTANCE`.
pub const MIN_REPOSITION_DISTANCE: i32 = 6;
pub const MAX_REPOSITION_DISTANCE: i32 = 7;

pub struct SpearAttack {
    speed_modifier_when_charging: f64,
    speed_modifier_when_repositioning: f64,
    target_in_range_radius_sq: f32,
}

impl SpearAttack {
    /// `new SpearAttack(speedModifierWhenCharging, speedModifierWhenRepositioning, targetInRangeRadius)`.
    pub fn new(
        speed_modifier_when_charging: f64,
        speed_modifier_when_repositioning: f64,
        target_in_range_radius: f32,
    ) -> BehaviorBase<SpearAttack> {
        BehaviorBase::new(
            entry_condition![SPEAR_STATUS => ValuePresent],
            SpearAttack {
                speed_modifier_when_charging,
                speed_modifier_when_repositioning,
                target_in_range_radius_sq: target_in_range_radius * target_in_range_radius,
            },
        )
    }
}

/// `getKineticWeaponUseDuration(mob)`: `KineticWeapon.computeDamageUseDuration` of the main hand weapon (0 without one).
fn get_kinetic_weapon_use_duration(mob: &dyn Entity) -> i32 {
    mob.get_item_by_slot(EquipmentSlot::MainHand)
        .get(&KINETIC_WEAPON)
        .map_or(0, |k| {
            k.delay_ticks.wrapping_add(
                k.damage_conditions
                    .as_ref()
                    .map_or(0, |c| c.max_duration_ticks),
            )
        })
}

impl Behavior for SpearAttack {
    fn check_extra_start_conditions(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        body.brain()
            .get_memory(&SPEAR_STATUS)
            .copied()
            .unwrap_or(SpearStatus::Approach)
            == SpearStatus::Charging
            && able_to_attack(body)
            && !using_item::is_using_item(body)
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(mob) = body.mob_mut() {
            mob.set_aggressive(true);
        }
        let duration = get_kinetic_weapon_use_duration(body);
        body.brain_mut().set_memory(&SPEAR_ENGAGE_TIME, duration);
        body.brain_mut().erase_memory(&SPEAR_CHARGE_POSITION);
        using_item::start_using_item(body, level, InteractionHand::MainHand);
    }

    fn can_still_use(
        &mut self,
        _level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain()
            .get_memory(&SPEAR_ENGAGE_TIME)
            .copied()
            .unwrap_or(0)
            > 0
            && able_to_attack(body)
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(target_dist_sqr) = target_distance_sqr(level, body) else {
            return;
        };
        let Some(target_id) = get_target(body) else {
            return;
        };
        let speed_modifier = f64::from(root_charge_speed_modifier(level, body));
        let mount_distance = if body.base().is_passenger() { 2 } else { 0 };
        let Some(target) = level.get_entity_or_recently_removed(target_id) else {
            return;
        };
        let look = PositionTracker::Entity(EntityTracker::new(target, true));
        let (target_pos, target_block) = (target.position(), target.block_position());
        body.brain_mut().set_memory(&LOOK_TARGET, look);
        let engage = body
            .brain()
            .get_memory(&SPEAR_ENGAGE_TIME)
            .copied()
            .unwrap_or(0);
        body.brain_mut()
            .set_memory(&SPEAR_ENGAGE_TIME, engage.wrapping_sub(1));
        let away_pos = body.brain().get_memory(&SPEAR_CHARGE_POSITION).copied();
        if let Some(away_pos) = away_pos {
            navigation::move_to(
                body,
                level,
                away_pos.x,
                away_pos.y,
                away_pos.z,
                speed_modifier * self.speed_modifier_when_repositioning,
            );
            if navigation::is_done(body, level) {
                body.brain_mut().erase_memory(&SPEAR_CHARGE_POSITION);
            }
        } else {
            navigation::move_to_entity(
                body,
                level,
                target_block,
                speed_modifier * self.speed_modifier_when_charging,
            );
            if target_dist_sqr < f64::from(self.target_in_range_radius_sq)
                || navigation::is_done(body, level)
            {
                let distance = target_dist_sqr.sqrt();
                let new_away_pos = land_random_pos::get_pos_away_range(
                    body,
                    level,
                    f64::from(MIN_REPOSITION_DISTANCE + mount_distance) - distance,
                    f64::from(MAX_REPOSITION_DISTANCE + mount_distance) - distance,
                    7,
                    target_pos,
                );
                body.brain_mut()
                    .set_memory_opt(&SPEAR_CHARGE_POSITION, new_away_pos);
            }
        }
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        navigation::stop(body, level);
        using_item::stop_using_item(body, level);
        body.brain_mut().erase_memory(&SPEAR_CHARGE_POSITION);
        body.brain_mut().erase_memory(&SPEAR_ENGAGE_TIME);
        body.brain_mut()
            .set_memory(&SPEAR_STATUS, SpearStatus::Retreat);
    }

    fn timed_out(&self, _timestamp: i64, _end_timestamp: i64) -> bool {
        false
    }
}
