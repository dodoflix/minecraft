//! `TradeWithVillager`: villagers gossip and pass food / requested items to each other.

use super::behavior_utils::{lock_gaze_and_walk_to_each_other, target_is_valid, throw_item};
use super::{Behavior, BehaviorBase, DEFAULT_DURATION};
use crate::entry_condition;
use crate::server::level::ServerLevel;
use crate::world::container::Container;
use crate::world::entity::Entity;
use crate::world::entity::ai::memory::EntityRef;
use crate::world::entity::ai::memory::memory_module_type::{
    INTERACTION_TARGET, NEAREST_VISIBLE_LIVING_ENTITIES,
};
use crate::world::entity::npc::villager::villager_profession as profession_data;
use crate::world::entity::npc::villager::{Villager, villager_mut, villager_ref};
use crate::world::item::ItemStack;
use mc_data::{EntityType, Item, VillagerProfession};
use mc_protocol::components::values::basic::VILLAGER_FOOD;

pub struct TradeWithVillager {
    trades: Vec<Item>,
}

impl TradeWithVillager {
    pub fn new() -> BehaviorBase<TradeWithVillager> {
        BehaviorBase::with_duration(
            entry_condition![INTERACTION_TARGET => ValuePresent, NEAREST_VISIBLE_LIVING_ENTITIES => ValuePresent],
            DEFAULT_DURATION,
            DEFAULT_DURATION,
            TradeWithVillager { trades: Vec::new() },
        )
    }
}

impl Behavior for TradeWithVillager {
    fn check_extra_start_conditions(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
    ) -> bool {
        target_is_valid(level, body, &INTERACTION_TARGET, |e| {
            e.get_type() == EntityType::VILLAGER
        })
    }

    fn can_still_use(
        &mut self,
        level: &mut ServerLevel,
        body: &mut dyn Entity,
        _timestamp: i64,
    ) -> bool {
        self.check_extra_start_conditions(level, body)
    }

    fn start(&mut self, level: &mut ServerLevel, my_body: &mut dyn Entity, _timestamp: i64) {
        let Some(&EntityRef(target_id)) = my_body.brain().get_memory(&INTERACTION_TARGET) else {
            return;
        };
        level.with_entity(target_id, |_, target| {
            lock_gaze_and_walk_to_each_other(my_body, target, 0.5, 2)
        });
        self.trades = level
            .get_entity(target_id)
            .and_then(villager_ref)
            .map(|target| figure_out_what_i_am_willing_to_trade(villager_mut(my_body), target))
            .unwrap_or_default();
    }

    fn tick(&mut self, level: &mut ServerLevel, body: &mut dyn Entity, timestamp: i64) {
        let Some(&EntityRef(target_id)) = body.brain().get_memory(&INTERACTION_TARGET) else {
            return;
        };
        let body_base_pos = body.position();
        let Some(dist) = level
            .get_entity(target_id)
            .map(|t| t.base().distance_to_sqr_vec(body_base_pos))
        else {
            return;
        };
        if dist > 5.0 {
            return;
        }
        level.with_entity(target_id, |level, target| {
            lock_gaze_and_walk_to_each_other(body, target, 0.5, 2);
            let target_villager = target
                .as_any_mut()
                .downcast_mut::<Villager>()
                .expect("villager");
            villager_mut(body).gossip(level, target_villager, timestamp);
            let is_farmer =
                villager_mut(body).get_villager_data().profession == VillagerProfession::FARMER;
            if villager_mut(body).has_excess_food()
                && (is_farmer || target_villager.wants_more_food())
            {
                throw_half_stack(
                    level,
                    villager_mut(body),
                    |s| s.get(&VILLAGER_FOOD).is_some(),
                    target_villager,
                );
            }
            if is_farmer && villager_mut(body).av.inventory.count_item(Item::WHEAT) > 64 / 2 {
                throw_half_stack(
                    level,
                    villager_mut(body),
                    |s| s.item() == Item::WHEAT,
                    target_villager,
                );
            }
            let has_any = !self.trades.is_empty()
                && villager_mut(body)
                    .av
                    .inventory
                    .get_items()
                    .iter()
                    .any(|s| !s.is_empty() && self.trades.contains(&s.item()));
            if has_any {
                let trades = self.trades.clone();
                throw_half_stack(
                    level,
                    villager_mut(body),
                    |s| trades.contains(&s.item()),
                    target_villager,
                );
            }
        });
    }

    fn stop(&mut self, _level: &mut ServerLevel, body: &mut dyn Entity, _timestamp: i64) {
        body.brain_mut().erase_memory(&INTERACTION_TARGET);
    }
}

fn figure_out_what_i_am_willing_to_trade(my_body: &Villager, target: &Villager) -> Vec<Item> {
    let target_items = profession_data::data(target.get_villager_data().profession).requested_items;
    let self_items = profession_data::data(my_body.get_villager_data().profession).requested_items;
    target_items
        .iter()
        .copied()
        .filter(|e| !self_items.contains(e))
        .collect()
}

fn throw_half_stack(
    level: &mut ServerLevel,
    villager: &mut Villager,
    predicate: impl Fn(&ItemStack) -> bool,
    target: &dyn Entity,
) {
    let mut to_throw = ItemStack::empty();
    let size = villager.av.inventory.get_container_size();
    for i in 0..size {
        let mut item_stack = villager.av.inventory.get_item(i).clone();
        if !item_stack.is_empty() && predicate(&item_stack) {
            let count = if item_stack.count() > item_stack.get_max_stack_size() / 2 {
                item_stack.count() / 2
            } else if item_stack.count() > 24 {
                item_stack.count() - 24
            } else {
                continue;
            };
            to_throw = item_stack.split(count);
            villager.av.inventory.set_item(i, item_stack);
            break;
        }
    }
    if !to_throw.is_empty() {
        throw_item(level, villager, to_throw, target.position());
    }
}
