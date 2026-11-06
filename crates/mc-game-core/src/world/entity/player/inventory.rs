//! `net.minecraft.world.entity.player.Inventory` (26.3 layout): 36 main slots (hotbar 0..9) in `items`, followed by
//! the equipment slots of `EQUIPMENT_SLOT_MAPPING` stored in the player's `EntityEquipment` (the inventory owns it,
//! vanilla's `PlayerEquipment` redirects `MAINHAND` to the selected hotbar slot, see [`Inventory::equipment_get`]).
//!
//! Vanilla's `Inventory` holds its `Player`; the Rust one does not (the player owns the inventory). Methods that
//! consult the owner take the needed facts as parameters (`has_infinite_materials`, the open menu's carried stack,
//! ...) and the ones that act on the owner (`placeItemBackInInventory`, `tick`, `dropAll`) return what the caller has
//! to do or live next to the player in `world::inventory::player_ops`.

use crate::world::container::{Container, empty_stack, helper, load_slots, save_slots};
use crate::world::entity::equipment::{EntityEquipment, EquipmentSlot};
use crate::world::inventory::MenuPlayer;
use crate::world::item::ItemStack;
use crate::world::item::ItemStackExt;
use mc_data::DataComponentType;
use mc_nbt::ListTag;
use std::any::Any;

pub const POP_TIME_DURATION: i32 = 5;
pub const INVENTORY_SIZE: usize = 36;
pub const SELECTION_SIZE: usize = 9;
pub const SLOT_OFFHAND: usize = 40;
pub const SLOT_BODY_ARMOR: usize = 41;
pub const SLOT_SADDLE: usize = 42;
pub const NOT_FOUND_INDEX: i32 = -1;

/// `EQUIPMENT_SLOT_MAPPING` (container slot -> equipment slot), in `Map.of` lookup form.
pub fn equipment_slot_for(slot: usize) -> Option<EquipmentSlot> {
    match slot {
        36 => Some(EquipmentSlot::Feet),
        37 => Some(EquipmentSlot::Legs),
        38 => Some(EquipmentSlot::Chest),
        39 => Some(EquipmentSlot::Head),
        40 => Some(EquipmentSlot::OffHand),
        41 => Some(EquipmentSlot::Body),
        42 => Some(EquipmentSlot::Saddle),
        _ => None,
    }
}

/// Number of entries of `EQUIPMENT_SLOT_MAPPING`.
const EQUIPMENT_SLOT_COUNT: usize = 7;

/// `Inventory.isHotbarSlot`.
pub fn is_hotbar_slot(slot: i32) -> bool {
    (0..9).contains(&slot)
}

/// `Inventory.isUsableForCrafting`.
pub fn is_usable_for_crafting(item: &ItemStack) -> bool {
    !item.is_damaged() && !item.is_enchanted() && !item.has(DataComponentType::CUSTOM_NAME)
}

#[derive(Clone, Debug)]
pub struct Inventory {
    items: Vec<ItemStack>,
    selected: usize,
    equipment: EntityEquipment,
    times_changed: i32,
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            items: vec![ItemStack::empty(); INVENTORY_SIZE],
            selected: 0,
            equipment: EntityEquipment::new(),
            times_changed: 0,
        }
    }

    pub fn get_selected_slot(&self) -> i32 {
        self.selected as i32
    }

    /// `setSelectedSlot`: vanilla throws `IllegalArgumentException("Invalid selected slot")` outside the hotbar.
    pub fn set_selected_slot(&mut self, selected: i32) -> Result<(), &'static str> {
        if !is_hotbar_slot(selected) {
            return Err("Invalid selected slot");
        }
        self.selected = selected as usize;
        Ok(())
    }

    pub fn get_selected_item(&self) -> &ItemStack {
        &self.items[self.selected]
    }

    pub fn get_selected_item_mut(&mut self) -> &mut ItemStack {
        &mut self.items[self.selected]
    }

    /// `setSelectedItem`: returns the replaced stack.
    pub fn set_selected_item(&mut self, stack: ItemStack) -> ItemStack {
        std::mem::replace(&mut self.items[self.selected], stack)
    }

    pub fn get_non_equipment_items(&self) -> &[ItemStack] {
        &self.items
    }

    pub fn get_non_equipment_items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    pub fn equipment(&self) -> &EntityEquipment {
        &self.equipment
    }

    pub fn equipment_mut(&mut self) -> &mut EntityEquipment {
        &mut self.equipment
    }

    /// `PlayerEquipment.get`: `MAINHAND` is the selected hotbar slot.
    pub fn equipment_get(&self, slot: EquipmentSlot) -> &ItemStack {
        if slot == EquipmentSlot::MainHand {
            self.get_selected_item()
        } else {
            self.equipment.get(slot)
        }
    }

    pub fn equipment_get_mut(&mut self, slot: EquipmentSlot) -> &mut ItemStack {
        if slot == EquipmentSlot::MainHand {
            &mut self.items[self.selected]
        } else {
            self.equipment.get_mut(slot)
        }
    }

    /// `PlayerEquipment.set`: returns the previous stack.
    pub fn equipment_set(&mut self, slot: EquipmentSlot, stack: ItemStack) -> ItemStack {
        if slot == EquipmentSlot::MainHand {
            self.set_selected_item(stack)
        } else {
            self.equipment.set(slot, stack)
        }
    }

    /// `PlayerEquipment.isEmpty`.
    pub fn equipment_is_empty(&self) -> bool {
        self.get_selected_item().is_empty() && self.equipment.is_empty()
    }

    fn has_remaining_space_for_item(&self, slot_stack: &ItemStack, new_stack: &ItemStack) -> bool {
        !slot_stack.is_empty()
            && ItemStack::is_same_item_same_components(slot_stack, new_stack)
            && slot_stack.is_stackable()
            && slot_stack.count() < self.get_max_stack_size_for(slot_stack)
    }

    pub fn get_free_slot(&self) -> i32 {
        self.items
            .iter()
            .position(ItemStack::is_empty)
            .map_or(-1, |i| i as i32)
    }

    pub fn add_and_pick_item(&mut self, stack: ItemStack) {
        let suitable = self.get_suitable_hotbar_slot();
        self.selected = suitable as usize;
        if !self.items[self.selected].is_empty() {
            let free = self.get_free_slot();
            if free != -1 {
                self.items[free as usize] = self.items[self.selected].clone();
            }
        }
        self.items[self.selected] = stack;
    }

    pub fn pick_slot(&mut self, slot: usize) {
        self.selected = self.get_suitable_hotbar_slot() as usize;
        self.items.swap(self.selected, slot);
    }

    pub fn find_slot_matching_item(&self, stack: &ItemStack) -> i32 {
        self.items
            .iter()
            .position(|i| !i.is_empty() && ItemStack::is_same_item_same_components(stack, i))
            .map_or(-1, |i| i as i32)
    }

    /// `findSlotMatchingCraftingIngredient(item, existingItem)`.
    pub fn find_slot_matching_crafting_ingredient(
        &self,
        item: mc_data::Item,
        existing: &ItemStack,
    ) -> i32 {
        self.items
            .iter()
            .position(|s| {
                !s.is_empty()
                    && s.is(item)
                    && is_usable_for_crafting(s)
                    && (existing.is_empty() || ItemStack::is_same_item_same_components(existing, s))
            })
            .map_or(-1, |i| i as i32)
    }

    pub fn get_suitable_hotbar_slot(&self) -> i32 {
        for slot in 0..9 {
            let index = (self.selected + slot) % 9;
            if self.items[index].is_empty() {
                return index as i32;
            }
        }
        for slot in 0..9 {
            let index = (self.selected + slot) % 9;
            if !self.items[index].is_enchanted() {
                return index as i32;
            }
        }
        self.selected as i32
    }

    /// `clearOrCountMatchingItems(predicate, countingOnly, amountToRemove, craftSlots)`; `carried` is the open menu's
    /// cursor stack (`player.containerMenu.getCarried()`).
    pub fn clear_or_count_matching_items(
        &mut self,
        predicate: &dyn Fn(&ItemStack) -> bool,
        counting_only: bool,
        amount_to_remove: i32,
        craft_slots: &mut dyn Container,
        carried: &mut ItemStack,
    ) -> i32 {
        let mut count = 0;
        count += helper::clear_or_count_matching_items(
            self,
            predicate,
            amount_to_remove - count,
            counting_only,
        );
        count += helper::clear_or_count_matching_items(
            craft_slots,
            predicate,
            amount_to_remove - count,
            counting_only,
        );
        count += helper::clear_or_count_matching_items_stack(
            carried,
            predicate,
            amount_to_remove - count,
            counting_only,
        );
        if !counting_only && carried.is_empty() {
            *carried = ItemStack::empty();
        }
        count
    }

    fn add_resource_any(&mut self, stack: &ItemStack) -> i32 {
        let mut slot = self.get_slot_with_remaining_space(stack);
        if slot == -1 {
            slot = self.get_free_slot();
        }
        if slot == -1 {
            stack.count()
        } else {
            self.add_resource(slot as usize, stack)
        }
    }

    fn add_resource(&mut self, slot: usize, stack: &ItemStack) -> i32 {
        let mut count = stack.count();
        if self.get_item(slot).is_empty() {
            self.set_item(slot, stack.copy_with_count(0));
        }
        let max_to_add =
            self.get_max_stack_size_for(self.get_item(slot)) - self.get_item(slot).count();
        let to_add = count.min(max_to_add);
        if to_add == 0 {
            count
        } else {
            count -= to_add;
            if let Some(in_slot) = self.get_item_mut(slot) {
                in_slot.grow(to_add);
            }
            count
        }
    }

    /// `getSlotWithRemainingSpace`.
    pub fn get_slot_with_remaining_space(&self, new_stack: &ItemStack) -> i32 {
        if self.has_remaining_space_for_item(self.get_item(self.selected), new_stack) {
            self.selected as i32
        } else if self.has_remaining_space_for_item(self.get_item(SLOT_OFFHAND), new_stack) {
            SLOT_OFFHAND as i32
        } else {
            self.items
                .iter()
                .position(|s| self.has_remaining_space_for_item(s, new_stack))
                .map_or(-1, |i| i as i32)
        }
    }

    /// `add(itemStack)`.
    pub fn add(&mut self, stack: &mut ItemStack, infinite_materials: bool) -> bool {
        self.add_at(-1, stack, infinite_materials)
    }

    /// `add(slot, itemStack)`: moves as much of `stack` as fits (shrinking it in place); returns whether anything moved.
    pub fn add_at(
        &mut self,
        mut slot: i32,
        stack: &mut ItemStack,
        infinite_materials: bool,
    ) -> bool {
        if stack.is_empty() {
            return false;
        }
        if stack.is_damaged() {
            if slot == -1 {
                slot = self.get_free_slot();
            }
            if slot >= 0 {
                self.items[slot as usize] = stack.copy_and_clear();
                true
            } else if infinite_materials {
                stack.set_count(0);
                true
            } else {
                false
            }
        } else {
            let mut last_size;
            loop {
                last_size = stack.count();
                let left = if slot == -1 {
                    self.add_resource_any(stack)
                } else {
                    self.add_resource(slot as usize, stack)
                };
                stack.set_count(left);
                if stack.is_empty() || stack.count() >= last_size {
                    break;
                }
            }
            if stack.count() == last_size && infinite_materials {
                stack.set_count(0);
                true
            } else {
                stack.count() < last_size
            }
        }
    }

    /// `removeFromSelected(all)`.
    pub fn remove_from_selected(&mut self, all: bool) -> ItemStack {
        let count = self.get_selected_item().count();
        if self.get_selected_item().is_empty() {
            ItemStack::empty()
        } else {
            let selected = self.selected;
            self.remove_item(selected, if all { count } else { 1 })
        }
    }

    pub fn contains(&self, search: &ItemStack) -> bool {
        self.contains_matching(&|s| {
            !s.is_empty() && ItemStack::is_same_item_same_components(s, search)
        })
    }

    pub fn contains_matching(&self, predicate: &dyn Fn(&ItemStack) -> bool) -> bool {
        (0..self.get_container_size()).any(|i| predicate(self.get_item(i)))
    }

    /// `replaceWith(other)`.
    pub fn replace_with(&mut self, other: &Inventory) {
        for i in 0..self.get_container_size() {
            self.set_item(i, other.get_item(i).clone());
        }
        self.selected = other.selected;
    }

    /// `save(output)`: the `Inventory` list tag (non-empty main slots only; equipment is saved with the entity).
    pub fn save(&self) -> ListTag {
        save_slots(self.items.iter().enumerate())
    }

    /// `load(input)`: undecodable entries and slots outside `0..36` are skipped.
    pub fn load(&mut self, list: &ListTag) {
        self.items.fill(ItemStack::empty());
        for (slot, stack) in load_slots(list) {
            if slot < self.items.len() {
                self.set_item(slot, stack);
            }
        }
    }

    pub fn get_times_changed(&self) -> i32 {
        self.times_changed
    }

    /// `dropAll()`: empties the main slots then the equipment map and returns the stacks in vanilla's drop order
    /// (main slots 0..36, then `EntityEquipment` in `EnumMap` order); the caller creates the item entities in this
    /// order (`createItemStackToDrop(stack, true, false)` consumes RNG per stack).
    pub fn drop_all(&mut self) -> Vec<ItemStack> {
        let mut drops: Vec<ItemStack> = self
            .items
            .iter_mut()
            .map(std::mem::take)
            .filter(|s| !s.is_empty())
            .collect();
        for slot in EquipmentSlot::VALUES {
            if slot != EquipmentSlot::MainHand {
                let stack = self.equipment.set(slot, ItemStack::empty());
                if !stack.is_empty() {
                    drops.push(stack);
                }
            }
        }
        drops
    }
}

impl Container for Inventory {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn get_container_size(&self) -> usize {
        self.items.len() + EQUIPMENT_SLOT_COUNT
    }

    fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
            && (36..self.get_container_size())
                .filter_map(equipment_slot_for)
                .all(|s| self.equipment.get(s).is_empty())
    }

    fn get_item(&self, slot: usize) -> &ItemStack {
        if slot < self.items.len() {
            &self.items[slot]
        } else {
            equipment_slot_for(slot).map_or(empty_stack(), |s| self.equipment.get(s))
        }
    }

    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        if slot < self.items.len() {
            Some(&mut self.items[slot])
        } else {
            equipment_slot_for(slot).map(|s| self.equipment.get_mut(s))
        }
    }

    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        if slot < self.items.len() {
            helper::remove_item(&mut self.items, slot, count)
        } else if let Some(s) = equipment_slot_for(slot) {
            let stack = self.equipment.get_mut(s);
            if stack.is_empty() {
                ItemStack::empty()
            } else {
                stack.split(count)
            }
        } else {
            ItemStack::empty()
        }
    }

    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        if slot < self.items.len() {
            std::mem::take(&mut self.items[slot])
        } else {
            equipment_slot_for(slot).map_or(ItemStack::empty(), |s| {
                self.equipment.set(s, ItemStack::empty())
            })
        }
    }

    /// Vanilla `Inventory.setItem` does not call `setChanged` and does not limit the stack size.
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if slot < self.items.len() {
            self.items[slot] = stack;
        } else if let Some(s) = equipment_slot_for(slot) {
            self.equipment.set(s, stack);
        }
    }

    fn clear_content(&mut self) {
        self.items.fill(ItemStack::empty());
        self.equipment.clear();
    }

    fn set_changed(&mut self) {
        self.times_changed = self.times_changed.wrapping_add(1);
    }

    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
}
