//! `InventoryChangeTrigger`: `inventory_changed`.

use super::prelude::*;
use mc_protocol::components::{PotionContents, PotionsPredicate, RegistrySet, StatePropertiesPredicate};
use crate::world::container::Container;
use crate::world::entity::player::inventory::Inventory;

/// `InventoryChangeTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct InventoryChangeTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub slots: Slots,
    pub items: Vec<ItemPredicate>,
}

impl TriggerInstance for InventoryChangeTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<InventoryChangeTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(InventoryChangeTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                slots: Slots::codec().optional_field_of_default("slots", Slots::any()),
                items: ItemPredicate::codec().list_of().optional_field_of_default("items", Vec::new()),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for InventoryChangeTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(InventoryChangeTriggerInstance);

crate::register_triggers!(InventoryChangeTriggerInstance {
    INVENTORY_CHANGED = "inventory_changed",
});

impl InventoryChangeTriggerInstance {
    /// `matches(inventory, changedItem, slotsFull, slotsEmpty, slotsOccupied)`.
    pub fn matches(&self, inventory: &Inventory, changed_item: &ItemStack, slots_full: i32, slots_empty: i32, slots_occupied: i32) -> bool {
        if !self.slots.matches(slots_full, slots_empty, slots_occupied) {
            false
        } else if self.items.is_empty() {
            true
        } else if self.items.len() != 1 {
            let mut predicates: Vec<&ItemPredicate> = self.items.iter().collect();
            for slot in 0..inventory.get_container_size() {
                if predicates.is_empty() {
                    return true;
                }
                let stack = inventory.get_item(slot);
                if !stack.is_empty() {
                    predicates.retain(|p| !p.test(stack));
                }
            }
            predicates.is_empty()
        } else {
            !changed_item.is_empty() && self.items[0].test(changed_item)
        }
    }
}

impl Trigger<InventoryChangeTriggerInstance> {
    /// `trigger(player, player.getInventory(), changedItem)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, changed_item: &ItemStack) {
        let matches = self.matches(&*level, &*player, changed_item);
        matches.award(level, player);
    }

    /// The listeners that match (the inventory is the player's own); nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, changed_item: &ItemStack) -> TriggerMatches {
        let inventory = &player.player.inventory;
        let (mut slots_full, mut slots_empty, mut slots_occupied) = (0, 0, 0);
        for slot in 0..inventory.get_container_size() {
            let stack = inventory.get_item(slot);
            if stack.is_empty() {
                slots_empty += 1;
            } else {
                slots_occupied += 1;
                if stack.count() >= stack.get_max_stack_size() {
                    slots_full += 1;
                }
            }
        }
        self.collect(level, player, |t| t.matches(inventory, changed_item, slots_full, slots_empty, slots_occupied))
    }
}

/// `InventoryChangeTrigger.TriggerInstance.Slots`.
#[derive(Clone, Debug, PartialEq)]
pub struct Slots {
    pub occupied: Ints,
    pub full: Ints,
    pub empty: Ints,
}

impl Slots {
    pub fn any() -> Slots {
        Slots { occupied: Ints::any(), full: Ints::any(), empty: Ints::any() }
    }

    pub fn codec() -> Codec<Slots> {
        mc_dfu::record!(Slots {
            occupied: Ints::codec().optional_field_of_default("occupied", Ints::any()),
            full: Ints::codec().optional_field_of_default("full", Ints::any()),
            empty: Ints::codec().optional_field_of_default("empty", Ints::any()),
        })
        .codec()
    }

    /// `matches(slotsFull, slotsEmpty, slotsOccupied)`.
    pub fn matches(&self, slots_full: i32, slots_empty: i32, slots_occupied: i32) -> bool {
        self.full.matches(slots_full) && self.empty.matches(slots_empty) && self.occupied.matches(slots_occupied)
    }
}
