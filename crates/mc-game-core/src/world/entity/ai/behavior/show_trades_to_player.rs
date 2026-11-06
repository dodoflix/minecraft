//! `ShowTradesToPlayer`: a villager holds up items it would trade for what the player holds.

use super::position_tracker::{EntityTracker, PositionTracker};
use super::{Behavior, BehaviorBase};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::EntityRef;
use crate::world::entity::ai::memory::memory_module_type::{INTERACTION_TARGET, LOOK_TARGET};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::npc::villager::villager_mut;
use crate::world::item::ItemStack;
use crate::world::item::trading::Merchant;
use mc_data::EntityType;

const MAX_LOOK_TIME: i32 = 900;
const STARTING_LOOK_TIME: i32 = 40;

pub struct ShowTradesToPlayer {
    player_item_stack: Option<ItemStack>,
    display_items: Vec<ItemStack>,
    cycle_counter: i32,
    display_index: usize,
    look_time: i32,
}

impl ShowTradesToPlayer {
    pub fn new(min_duration: i32, max_duration: i32) -> BehaviorBase<ShowTradesToPlayer> {
        BehaviorBase::with_duration(
            entry_condition![INTERACTION_TARGET => ValuePresent],
            min_duration,
            max_duration,
            ShowTradesToPlayer {
                player_item_stack: None,
                display_items: Vec::new(),
                cycle_counter: 0,
                display_index: 0,
                look_time: 0,
            },
        )
    }

    fn find_items_to_display(
        &mut self,
        level: &mut ServerLevel,
        player_id: crate::world::entity::EntityId,
        villager: &mut dyn Entity,
    ) {
        let mut changed = false;
        let current_player_item_stack = level
            .get_entity(player_id)
            .map(|p| p.get_item_by_slot(EquipmentSlot::MainHand))
            .unwrap_or_else(ItemStack::empty);
        if self
            .player_item_stack
            .as_ref()
            .is_none_or(|s| !ItemStack::is_same_item(s, &current_player_item_stack))
        {
            self.player_item_stack = Some(current_player_item_stack);
            changed = true;
            self.display_items.clear();
        }
        if changed
            && self
                .player_item_stack
                .as_ref()
                .is_some_and(|s| !s.is_empty())
        {
            self.update_display_items(level, villager);
            if !self.display_items.is_empty() {
                self.look_time = MAX_LOOK_TIME;
                self.display_first_item(level, villager);
            }
        }
    }

    fn display_first_item(&self, level: &mut ServerLevel, villager: &mut dyn Entity) {
        display_as_held_item(level, villager, self.display_items[0].clone());
    }

    fn update_display_items(&mut self, level: &mut ServerLevel, villager: &mut dyn Entity) {
        let player_stack = self
            .player_item_stack
            .clone()
            .unwrap_or_else(ItemStack::empty);
        let offers = villager_mut(villager).get_offers(level).clone();
        for offer in offers.0.iter() {
            if !offer.is_out_of_stock()
                && (ItemStack::is_same_item(&player_stack, &offer.get_cost_a())
                    || ItemStack::is_same_item(&player_stack, &offer.get_cost_b()))
            {
                self.display_items.push(offer.assemble());
            }
        }
    }

    fn look_at_target(
        &self,
        body: &mut dyn Entity,
        level: &ServerLevel,
    ) -> Option<crate::world::entity::EntityId> {
        let EntityRef(target) = *body.brain().get_memory(&INTERACTION_TARGET)?;
        let entity = level.get_entity(target)?;
        let tracker = PositionTracker::Entity(EntityTracker::new(entity, true));
        body.brain_mut().set_memory(&LOOK_TARGET, tracker);
        Some(target)
    }

    fn display_cycling_items(&mut self, level: &mut ServerLevel, villager: &mut dyn Entity) {
        if self.display_items.len() >= 2 {
            self.cycle_counter += 1;
            if self.cycle_counter >= 40 {
                self.display_index += 1;
                self.cycle_counter = 0;
                if self.display_index > self.display_items.len() - 1 {
                    self.display_index = 0;
                }
                display_as_held_item(
                    level,
                    villager,
                    self.display_items[self.display_index].clone(),
                );
            }
        }
    }
}

fn clear_held_item(level: &mut ServerLevel, body: &mut dyn Entity) {
    body.set_item_slot(level, EquipmentSlot::MainHand, ItemStack::empty());
    body.set_drop_chance(EquipmentSlot::MainHand, 0.085);
}

fn display_as_held_item(level: &mut ServerLevel, body: &mut dyn Entity, item_stack: ItemStack) {
    body.set_item_slot(level, EquipmentSlot::MainHand, item_stack);
    body.set_drop_chance(EquipmentSlot::MainHand, 0.0);
}

impl Behavior for ShowTradesToPlayer {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        let Some(&EntityRef(target_id)) = body.brain().get_memory(&INTERACTION_TARGET) else {
            return false;
        };
        let Some(target) = level.get_entity(target_id) else {
            return false;
        };
        target.get_type() == EntityType::PLAYER
            && body.is_alive()
            && target.is_alive()
            && !body.is_baby()
            && body.base().distance_to_sqr_vec(target.position()) <= 17.0
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.check_extra_start_conditions(level, body)
            && self.look_time > 0
            && body.brain().get_memory(&INTERACTION_TARGET).is_some()
    }

    fn start(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        self.look_at_target(body, level);
        self.cycle_counter = 0;
        self.display_index = 0;
        self.look_time = STARTING_LOOK_TIME;
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        let Some(target) = self.look_at_target(body, level) else {
            return;
        };
        self.find_items_to_display(level, target, body);
        if !self.display_items.is_empty() {
            self.display_cycling_items(level, body);
        } else {
            clear_held_item(level, body);
            self.look_time = self.look_time.min(STARTING_LOOK_TIME);
        }
        self.look_time -= 1;
    }

    fn stop(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&INTERACTION_TARGET);
        clear_held_item(level, body);
        self.player_item_stack = None;
    }
}
