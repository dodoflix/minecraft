//! `SpearRetreat`: back away from the target after a charge.

use super::spear_approach::{
    able_to_attack, get_target, root_charge_speed_modifier, target_distance_sqr,
};
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase};
use mc_game_core::world::entity::ai::memory::SpearStatus;
use mc_game_core::world::entity::ai::memory::memory_module_type::{
    LOOK_TARGET, SPEAR_FLEEING_POSITION, SPEAR_FLEEING_TIME, SPEAR_STATUS,
};
use mc_game_core::world::entity::ai::util::land_random_pos;
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::living_entity::using_item;
use mc_game_core::world::entity::mob::navigation;

/// `MIN_COOLDOWN_DISTANCE` / `MAX_COOLDOWN_DISTANCE` / `MAX_FLEEING_TIME`.
pub const MIN_COOLDOWN_DISTANCE: i32 = 9;
pub const MAX_COOLDOWN_DISTANCE: i32 = 11;
pub const MAX_FLEEING_TIME: i32 = 100;

pub struct SpearRetreat {
    speed_modifier_when_repositioning: f64,
}

impl SpearRetreat {
    /// `new SpearRetreat(speedModifierWhenRepositioning)`.
    pub fn new(speed_modifier_when_repositioning: f64) -> BehaviorBase<SpearRetreat> {
        BehaviorBase::with_timeout(
            entry_condition![SPEAR_STATUS => ValuePresent],
            MAX_FLEEING_TIME,
            SpearRetreat {
                speed_modifier_when_repositioning,
            },
        )
    }
}

impl Behavior for SpearRetreat {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        if !(able_to_attack(body) && !using_item::is_using_item(body)) {
            return false;
        }
        if body
            .brain()
            .get_memory(&SPEAR_STATUS)
            .copied()
            .unwrap_or(SpearStatus::Approach)
            != SpearStatus::Retreat
        {
            return false;
        }
        let Some(target_dist_sqr) = target_distance_sqr(level, body) else {
            return false;
        };
        let Some(target_pos) = get_target(body)
            .and_then(|t| level.get_entity_or_recently_removed(t))
            .map(|t| t.position())
        else {
            return false;
        };
        let mount_distance = if body.base().is_passenger() { 2 } else { 0 };
        let distance = target_dist_sqr.sqrt();
        let away_pos = land_random_pos::get_pos_away_range(
            body,
            level,
            f64::max(
                0.0,
                f64::from(MIN_COOLDOWN_DISTANCE + mount_distance) - distance,
            ),
            f64::max(
                1.0,
                f64::from(MAX_COOLDOWN_DISTANCE + mount_distance) - distance,
            ),
            7,
            target_pos,
        );
        match away_pos {
            None => false,
            Some(pos) => {
                body.brain_mut().set_memory(&SPEAR_FLEEING_POSITION, pos);
                true
            }
        }
    }

    fn start(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if let Some(mob) = body.mob_mut() {
            mob.set_aggressive(true);
        }
        body.brain_mut().set_memory(&SPEAR_FLEEING_TIME, 0);
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain()
            .get_memory(&SPEAR_FLEEING_TIME)
            .copied()
            .unwrap_or(MAX_FLEEING_TIME)
            < MAX_FLEEING_TIME
            && body.brain().get_memory(&SPEAR_FLEEING_POSITION).is_some()
            && !navigation::is_done(body, level)
            && able_to_attack(body)
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(target) = get_target(body).and_then(|t| level.get_entity_or_recently_removed(t))
        else {
            return;
        };
        let speed_modifier = root_charge_speed_modifier(level, body);
        let look = PositionTracker::Entity(EntityTracker::new(target, true));
        body.brain_mut().set_memory(&LOOK_TARGET, look);
        let time = body
            .brain()
            .get_memory(&SPEAR_FLEEING_TIME)
            .copied()
            .unwrap_or(0);
        body.brain_mut()
            .set_memory(&SPEAR_FLEEING_TIME, time.wrapping_add(1));
        if let Some(flee_pos) = body.brain().get_memory(&SPEAR_FLEEING_POSITION).copied() {
            navigation::move_to(
                body,
                level,
                flee_pos.x,
                flee_pos.y,
                flee_pos.z,
                f64::from(speed_modifier) * self.speed_modifier_when_repositioning,
            );
        }
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        navigation::stop(body, level);
        if let Some(mob) = body.mob_mut() {
            mob.set_aggressive(false);
        }
        using_item::stop_using_item(body, level);
        body.brain_mut().erase_memory(&SPEAR_FLEEING_TIME);
        body.brain_mut().erase_memory(&SPEAR_FLEEING_POSITION);
        body.brain_mut().erase_memory(&SPEAR_STATUS);
    }
}
