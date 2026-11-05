//! `InventoryMenu` (the player's own menu, container id 0).

use super::{
    AbstractContainerMenu, ArmorSlot, ContainerKey, MenuBase, MenuPlayer, OffhandSlot, Prediction, ResultContainer, Slot, TransientCraftingContainer,
    result_slot,
};
use crate::world::entity::equipment::{EquipmentSlot, EquipmentSlotType};
use crate::world::item::ItemStack;
use std::any::Any;

pub const CONTAINER_ID: i32 = 0;
pub const RESULT_SLOT: usize = 0;
pub const CRAFT_SLOT_START: usize = 1;
pub const CRAFT_SLOT_COUNT: usize = 4;
pub const CRAFT_SLOT_END: usize = 5;
pub const ARMOR_SLOT_START: usize = 5;
pub const ARMOR_SLOT_COUNT: usize = 4;
pub const ARMOR_SLOT_END: usize = 9;
pub const INV_SLOT_START: usize = 9;
pub const INV_SLOT_END: usize = 36;
pub const USE_ROW_SLOT_START: usize = 36;
pub const USE_ROW_SLOT_END: usize = 45;
pub const SHIELD_SLOT: usize = 45;

const SLOT_IDS: [EquipmentSlot; 4] = [EquipmentSlot::Head, EquipmentSlot::Chest, EquipmentSlot::Legs, EquipmentSlot::Feet];

/// Menu-local container indices.
const CRAFT_LOCAL: usize = 0;
const RESULT_LOCAL: usize = 1;

pub struct InventoryMenu {
    base: MenuBase,
    pub active: bool,
}

/// `InventoryMenu.isHotbarSlot`.
pub fn is_hotbar_slot(slot: usize) -> bool {
    (36..45).contains(&slot) || slot == 45
}

impl InventoryMenu {
    pub fn new(active: bool) -> Self {
        let mut base = MenuBase::new(None, CONTAINER_ID);
        let craft = base.add_local(Box::new(TransientCraftingContainer::new(2, 2)));
        let result = base.add_local(Box::new(ResultContainer::new()));
        debug_assert_eq!((craft, result), (CRAFT_LOCAL, RESULT_LOCAL));
        base.add_slot(result_slot(RESULT_LOCAL, CRAFT_LOCAL, 154, 28));
        // `addCraftingGridSlots(98, 18)` (width == height == 2)
        for y in 0..2 {
            for x in 0..2 {
                base.add_slot(Slot::new(ContainerKey::Local(CRAFT_LOCAL), (x + y * 2) as usize, 98 + x * 18, 18 + y * 18));
            }
        }
        for (i, slot) in SLOT_IDS.into_iter().enumerate() {
            base.add_slot(Slot::with_behavior(ContainerKey::PlayerInventory, 39 - i, 8, 8 + i as i32 * 18, Box::new(ArmorSlot::new(slot))));
        }
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        base.add_slot(Slot::with_behavior(ContainerKey::PlayerInventory, 40, 77, 62, Box::new(OffhandSlot)));
        Self { base, active }
    }

    pub fn get_craft_slots(&self) -> &TransientCraftingContainer {
        self.base.locals[CRAFT_LOCAL].as_any().downcast_ref().expect("craft slots")
    }

    pub fn get_craft_slots_mut(&mut self) -> &mut TransientCraftingContainer {
        self.base.locals[CRAFT_LOCAL].as_any_mut().downcast_mut().expect("craft slots")
    }

    pub fn get_result_slot(&self) -> &Slot {
        &self.base.slots[0]
    }

    /// `CraftingMenu.slotChangedCraftingGrid(this, level, owner, craftSlots, resultSlots, null)`.
    fn slot_changed_crafting_grid(&mut self) {
        let result = ItemStack::empty();
        self.base.locals[RESULT_LOCAL].set_item(0, result.clone());
        self.base.set_remote_slot(0, &result);
        self.base.send_slot_change_now(0, result);
    }
}

impl AbstractContainerMenu for InventoryMenu {
    fn base(&self) -> &MenuBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MenuBase {
        &mut self.base
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, _container: &ContainerKey) {
        // Vanilla only recomputes when `owner.level() instanceof ServerLevel`, i.e. always on the server.
        if player.is_server_player() {
            self.slot_changed_crafting_grid();
        }
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        self.base.locals[RESULT_LOCAL].clear_content();
        self.clear_container(player, CRAFT_LOCAL);
    }

    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        let mut clicked = ItemStack::empty();
        let has = self.base.with_slot(player, slot_index, |s, e| s.has_item(e));
        if has {
            // Work on a copy: Java mutates the slot's own object, but the branch checks below read the slot (e.g.
            // `!slots.get(45).hasItem()`), which must still hold the stack; the remainder is written back afterwards.
            let mut stack = self.base.with_slot(player, slot_index, |s, e| s.get_item(e));
            clicked = stack.copy();
            let eq_slot = player.get_equipment_slot_for_item(&clicked);
            let moved = if slot_index == 0 {
                let moved = self.move_item_stack_to(player, &mut stack, 9, 45, true);
                if moved {
                    self.base.with_slot(player, 0, |s, e| s.on_quick_craft(e, &stack, &clicked));
                }
                moved
            } else if (1..5).contains(&slot_index) || (5..9).contains(&slot_index) {
                self.move_item_stack_to(player, &mut stack, 9, 45, false)
            } else if eq_slot.get_type() == EquipmentSlotType::HumanoidArmor && !self.base.with_slot(player, (8 - eq_slot.get_index()) as usize, |s, e| s.has_item(e)) {
                let pos = (8 - eq_slot.get_index()) as usize;
                self.move_item_stack_to(player, &mut stack, pos, pos + 1, false)
            } else if eq_slot == EquipmentSlot::OffHand && !self.base.with_slot(player, 45, |s, e| s.has_item(e)) {
                self.move_item_stack_to(player, &mut stack, 45, 46, false)
            } else if (9..36).contains(&slot_index) {
                self.move_item_stack_to(player, &mut stack, 36, 45, false)
            } else if (36..45).contains(&slot_index) {
                self.move_item_stack_to(player, &mut stack, 9, 36, false)
            } else {
                self.move_item_stack_to(player, &mut stack, 9, 45, false)
            };
            if !moved {
                // `moveItemStackTo` returned false: nothing was changed.
                return ItemStack::empty();
            }
            if stack.is_empty() {
                self.base.with_slot(player, slot_index, |s, e| s.set_by_player_with(e, ItemStack::empty(), &clicked));
            } else {
                let rest = stack.clone();
                self.base.with_slot(player, slot_index, |s, e| {
                    s.with_item_mut(e, |t| *t = rest);
                    s.set_changed(e);
                });
            }
            if stack.count() == clicked.count() {
                return ItemStack::empty();
            }
            self.base.with_slot(player, slot_index, |s, e| s.on_take(e, &stack));
            self.flush_container_events(player);
            if slot_index == 0 {
                player.drop(stack, false, Prediction::Predicted);
            }
        }
        clicked
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, target: &Slot) -> bool {
        !matches!(target.container, ContainerKey::Local(i) if i == RESULT_LOCAL)
    }
}
