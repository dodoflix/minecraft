//! `CrossbowAttack`: a mob holding a crossbow charges, waits and shoots at its attack target.

use mc_core::random::RandomSource;
use mc_data::Item;
use mc_game_core::entry_condition;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::EntityId;
use mc_game_core::world::entity::ai::behavior::position_tracker::{EntityTracker, PositionTracker};
use mc_game_core::world::entity::ai::behavior::{Behavior, BehaviorBase, behavior_utils};
use mc_game_core::world::entity::ai::memory::memory_module_type::{ATTACK_TARGET, LOOK_TARGET};
use mc_game_core::world::entity::entity::Entity;
use mc_game_core::world::entity::living_entity::using_item;
use mc_game_core::world::item::crossbow_item;
use mc_game_core::world::item::projectile_weapon_item::get_weapon_holding_hand;

/// `TIMEOUT`.
const TIMEOUT: i32 = 1200;

/// `CrossbowAttack.CrossbowState`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CrossbowState {
    Uncharged,
    Charging,
    Charged,
    ReadyToAttack,
}

pub struct CrossbowAttack {
    attack_delay: i32,
    crossbow_state: CrossbowState,
}

impl CrossbowAttack {
    /// `new CrossbowAttack()`.
    pub fn new() -> BehaviorBase<CrossbowAttack> {
        BehaviorBase::with_timeout(
            entry_condition![LOOK_TARGET => Registered, ATTACK_TARGET => ValuePresent],
            TIMEOUT,
            CrossbowAttack {
                attack_delay: 0,
                crossbow_state: CrossbowState::Uncharged,
            },
        )
    }

    fn crossbow_attack(&mut self, level: &mut ServerLevel, body: &mut dyn Entity) {
        match self.crossbow_state {
            CrossbowState::Uncharged => {
                let hand = get_weapon_holding_hand(&*body, Item::CROSSBOW);
                using_item::start_using_item(body, level, hand);
                self.crossbow_state = CrossbowState::Charging;
                set_charging_crossbow(body, true);
            }
            CrossbowState::Charging => {
                if !using_item::is_using_item(body) {
                    self.crossbow_state = CrossbowState::Uncharged;
                }
                let pull_time = using_item::get_ticks_using_item(body);
                let use_item = using_item::get_use_item(body);
                if pull_time >= crossbow_item::get_charge_duration(level, &use_item, body) {
                    using_item::release_using_item(body, level);
                    self.crossbow_state = CrossbowState::Charged;
                    self.attack_delay = 20 + body.base_mut().random.next_int_bounded(20);
                    set_charging_crossbow(body, false);
                }
            }
            CrossbowState::Charged => {
                self.attack_delay -= 1;
                if self.attack_delay == 0 {
                    self.crossbow_state = CrossbowState::ReadyToAttack;
                }
            }
            CrossbowState::ReadyToAttack => {
                let target = get_attack_target(body);
                if let Some(target) = target {
                    body.perform_ranged_attack(level, target, 1.0);
                }
                self.crossbow_state = CrossbowState::Uncharged;
            }
        }
    }
}

fn set_charging_crossbow(body: &mut dyn Entity, charging: bool) {
    body.set_charging_crossbow(charging);
}

fn get_attack_target(body: &dyn Entity) -> Option<EntityId> {
    body.brain().get_memory(&ATTACK_TARGET).map(|r| r.0)
}

impl Behavior for CrossbowAttack {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let Some(attack_target) = get_attack_target(body) else {
            return false;
        };
        let holding_crossbow = body
            .get_item_by_slot(mc_game_core::world::entity::equipment::EquipmentSlot::MainHand)
            .item()
            == Item::CROSSBOW
            || body
                .get_item_by_slot(mc_game_core::world::entity::equipment::EquipmentSlot::OffHand)
                .item()
                == Item::CROSSBOW;
        holding_crossbow
            && behavior_utils::can_see(level, body, attack_target)
            && behavior_utils::is_within_attack_range(level, body, attack_target, 0)
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        body.brain().has_memory_value(ATTACK_TARGET)
            && self.check_extra_start_conditions(level, body)
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(target) = get_attack_target(body) else {
            return;
        };
        // `lookAtTarget`: `LOOK_TARGET = new EntityTracker(target, true)`
        if let Some(t) = level.get_entity_or_recently_removed(target) {
            body.brain_mut().set_memory(
                &LOOK_TARGET,
                PositionTracker::Entity(EntityTracker::new(t, true)),
            );
        }
        self.crossbow_attack(level, body);
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        if using_item::is_using_item(body) {
            using_item::stop_using_item(body, level);
        }
        if body
            .get_item_by_slot(mc_game_core::world::entity::equipment::EquipmentSlot::MainHand)
            .item()
            == Item::CROSSBOW
            || body
                .get_item_by_slot(mc_game_core::world::entity::equipment::EquipmentSlot::OffHand)
                .item()
                == Item::CROSSBOW
        {
            set_charging_crossbow(body, false);
            // `body.getUseItem().set(CHARGED_PROJECTILES, EMPTY)`: after `stopUsingItem()` the use item is `ItemStack.EMPTY`,
            // so this never touches the crossbow in the hand
        }
    }
}
