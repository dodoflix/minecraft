//! `ItemCombinerMenu` + `ItemCombinerMenuSlotDefinition`: the base of the anvil and the smithing table.
//!
//! The Java abstract class becomes [`ItemCombinerMenuCore`] (the slots/containers it builds, owned by the concrete menu next
//! to its own state) plus the [`ItemCombinerMenu`] trait (`createResult`, `isValidBlock`, `canMoveIntoInputSlots` are the
//! abstract/overridable methods, the shared `slotsChanged`/`removed`/`stillValid`/`quickMoveStack` are provided methods the
//! concrete menu's `AbstractContainerMenu` impl forwards to). The result slot's `mayPickup`/`onTake` overrides cannot reach
//! the menu from inside a slot: the concrete menu hands its result [`SlotBehavior`] (sharing the menu's state through an
//! `Rc`) to [`ItemCombinerMenuCore::new`].

use super::{
    AbstractContainerMenu, ContainerKey, MenuBase, MenuPlayer, MenuSimpleContainer, Slot, SlotBehavior, SlotEnv, finish_quick_move,
};
use crate::world::container::Container;
use crate::world::inventory::ContainerLevelAccess;
use crate::world::item::ItemStack;
use mc_data::MenuType;
use mc_data::block::BlockState;
use std::any::Any;

/// `Predicate<ItemStack> mayPlace` of a slot definition.
pub type MayPlace = Box<dyn Fn(&ItemStack) -> bool>;

/// `ItemCombinerMenuSlotDefinition.SlotDefinition`.
pub struct SlotDefinition {
    pub slot_index: usize,
    pub x: i32,
    pub y: i32,
    pub may_place: MayPlace,
}

/// `ItemCombinerMenuSlotDefinition`.
pub struct ItemCombinerMenuSlotDefinition {
    slots: Vec<SlotDefinition>,
    result_slot: SlotDefinition,
}

/// `ItemCombinerMenuSlotDefinition.Builder`.
#[derive(Default)]
pub struct ItemCombinerMenuSlotDefinitionBuilder {
    input_slots: Vec<SlotDefinition>,
    result_slot: Option<SlotDefinition>,
}

impl ItemCombinerMenuSlotDefinition {
    pub fn create() -> ItemCombinerMenuSlotDefinitionBuilder {
        ItemCombinerMenuSlotDefinitionBuilder::default()
    }

    pub fn get_slot(&self, index: usize) -> &SlotDefinition {
        &self.slots[index]
    }
    pub fn get_result_slot(&self) -> &SlotDefinition {
        &self.result_slot
    }
    pub fn get_slots(&self) -> &[SlotDefinition] {
        &self.slots
    }
    pub fn get_num_of_input_slots(&self) -> usize {
        self.slots.len()
    }
    pub fn get_result_slot_index(&self) -> usize {
        self.get_num_of_input_slots()
    }
}

impl ItemCombinerMenuSlotDefinitionBuilder {
    pub fn with_slot(mut self, slot_index: usize, x: i32, y: i32, may_place: impl Fn(&ItemStack) -> bool + 'static) -> Self {
        self.input_slots.push(SlotDefinition { slot_index, x, y, may_place: Box::new(may_place) });
        self
    }

    pub fn with_result_slot(mut self, slot_index: usize, x: i32, y: i32) -> Self {
        self.result_slot = Some(SlotDefinition { slot_index, x, y, may_place: Box::new(|_| false) });
        self
    }

    /// `build()`: panics with vanilla's `IllegalArgumentException` messages.
    pub fn build(self) -> ItemCombinerMenuSlotDefinition {
        let input_count = self.input_slots.len();
        for (i, def) in self.input_slots.iter().enumerate() {
            assert!(def.slot_index == i, "Expected input slots to have continous indexes");
        }
        let result = self.result_slot;
        assert!(result.as_ref().is_none_or(|r| r.slot_index == input_count), "Expected result slot index to follow last input slot");
        match result {
            Some(result) if input_count > 0 => ItemCombinerMenuSlotDefinition { slots: self.input_slots, result_slot: result },
            _ => panic!("Need to define both inputSlots and resultSlot"),
        }
    }
}

/// `ItemCombinerMenu.resultSlots`: a `ResultContainer` whose `setChanged()` calls `slotsChanged(this)` (only an explicit
/// `setChanged`, `setItem` does not notify).
#[derive(Default)]
struct CombinerResultContainer {
    item: ItemStack,
    pending: u32,
}

impl Container for CombinerResultContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        1
    }
    fn is_empty(&self) -> bool {
        self.item.is_empty()
    }
    fn get_item(&self, _slot: usize) -> &ItemStack {
        &self.item
    }
    fn get_item_mut(&mut self, _slot: usize) -> Option<&mut ItemStack> {
        Some(&mut self.item)
    }
    fn remove_item(&mut self, _slot: usize, _count: i32) -> ItemStack {
        std::mem::take(&mut self.item)
    }
    fn remove_item_no_update(&mut self, _slot: usize) -> ItemStack {
        std::mem::take(&mut self.item)
    }
    fn set_item(&mut self, _slot: usize, stack: ItemStack) {
        self.item = stack;
    }
    fn clear_content(&mut self) {
        self.item = ItemStack::empty();
    }
    fn set_changed(&mut self) {
        self.pending += 1;
    }
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
    fn take_menu_notifications(&mut self) -> u32 {
        std::mem::take(&mut self.pending)
    }
}

/// An input slot whose `mayPlace` is the definition's predicate.
struct CombinerInputSlot(MayPlace);

impl SlotBehavior for CombinerInputSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        (self.0)(stack)
    }
}

/// The state `ItemCombinerMenu` itself owns: the menu base with its slots and the access of the block.
pub struct ItemCombinerMenuCore {
    pub base: MenuBase,
    pub access: ContainerLevelAccess,
    /// `inputSlots` (a `MenuBase::locals` index).
    pub input_local: usize,
    /// `resultSlots` (a `MenuBase::locals` index).
    pub result_local: usize,
    result_slot_index: usize,
}

const INVENTORY_SLOTS: usize = 27;
const USE_ROW_SLOTS: usize = 9;

impl ItemCombinerMenuCore {
    /// The constructor: `createInputSlots`, `createResultSlot` (with the concrete menu's `mayPickup`/`onTake` behavior),
    /// `addStandardInventorySlots(inventory, 8, 84)`.
    pub fn new(
        menu_type: Option<MenuType>,
        container_id: i32,
        access: ContainerLevelAccess,
        definition: ItemCombinerMenuSlotDefinition,
        result_behavior: Box<dyn SlotBehavior>,
    ) -> Self {
        let mut base = MenuBase::new(menu_type, container_id);
        let input_local = base.add_local(Box::new(MenuSimpleContainer::new(definition.get_num_of_input_slots())));
        let result_local = base.add_local(Box::new(CombinerResultContainer::default()));
        let result_slot_index = definition.get_result_slot_index();
        let ItemCombinerMenuSlotDefinition { slots, result_slot } = definition;
        for def in slots {
            base.add_slot(Slot::with_behavior(ContainerKey::Local(input_local), def.slot_index, def.x, def.y, Box::new(CombinerInputSlot(def.may_place))));
        }
        base.add_slot(Slot::with_behavior(ContainerKey::Local(result_local), result_slot.slot_index, result_slot.x, result_slot.y, result_behavior));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self { base, access, input_local, result_local, result_slot_index }
    }

    /// `getResultSlot()`.
    pub fn get_result_slot(&self) -> usize {
        self.result_slot_index
    }
    fn inventory_slot_start(&self) -> usize {
        self.get_result_slot() + 1
    }
    fn inventory_slot_end(&self) -> usize {
        self.inventory_slot_start() + INVENTORY_SLOTS
    }
    fn use_row_start(&self) -> usize {
        self.inventory_slot_end()
    }
    fn use_row_end(&self) -> usize {
        self.use_row_start() + USE_ROW_SLOTS
    }
}

/// `ItemCombinerMenu`'s abstract and overridable methods plus the algorithms the subclass forwards to.
pub trait ItemCombinerMenu: AbstractContainerMenu {
    fn core(&self) -> &ItemCombinerMenuCore;
    fn core_mut(&mut self) -> &mut ItemCombinerMenuCore;

    /// `isValidBlock(state)`.
    fn is_valid_block(&self, state: BlockState) -> bool;
    /// `createResult()` (the player is `this.player`).
    fn create_result(&mut self, player: &mut dyn MenuPlayer);
    /// `canMoveIntoInputSlots(stack)`.
    fn can_move_into_input_slots(&self, _stack: &ItemStack) -> bool {
        true
    }

    /// `slotsChanged(container)`: `super.slotsChanged` (broadcast), then `createResult` for the input container.
    fn combiner_slots_changed(&mut self, player: &mut dyn MenuPlayer, container: &ContainerKey) {
        self.core_mut().base.broadcast_changes(player);
        if matches!(container, ContainerKey::Local(i) if *i == self.core().input_local) {
            self.create_result(player);
        }
    }

    /// `removed(player)`.
    fn combiner_removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        if self.core().access.pos().is_some() {
            let input = self.core().input_local;
            self.clear_container(player, input);
        }
    }

    /// `stillValid(player)`: the access block must satisfy `isValidBlock` and be within 4 blocks; `NULL` access (and a
    /// player without a level) is valid.
    fn combiner_still_valid(&self, player: &dyn MenuPlayer) -> bool {
        match (self.core().access.pos(), player.level_ref()) {
            (Some(pos), Some(level)) => {
                use crate::world::level::BlockGetter;
                self.is_valid_block(level.get_block_state(pos)) && player.is_within_block_interaction_range(pos, 4.0)
            }
            _ => true,
        }
    }

    /// `quickMoveStack(player, slotIndex)`.
    fn combiner_quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack
    where
        Self: Sized,
    {
        if !self.core_mut().base.with_slot(player, slot_index, |s, e| s.has_item(e)) {
            return ItemStack::empty();
        }
        let mut stack = self.core_mut().base.with_slot(player, slot_index, |s, e| s.get_item(e));
        let clicked = stack.copy();
        let (inventory_start, inventory_end, use_row_start, use_row_end, result_slot) = {
            let c = self.core();
            (c.inventory_slot_start(), c.inventory_slot_end(), c.use_row_start(), c.use_row_end(), c.get_result_slot())
        };
        if slot_index == result_slot {
            if !self.move_item_stack_to(player, &mut stack, inventory_start, use_row_end, true) {
                return ItemStack::empty();
            }
            self.core_mut().base.with_slot(player, slot_index, |s, e| s.on_quick_craft(e, &stack, &clicked));
        } else if slot_index < result_slot {
            if !self.move_item_stack_to(player, &mut stack, inventory_start, use_row_end, false) {
                return ItemStack::empty();
            }
        } else if self.can_move_into_input_slots(&stack) && slot_index >= inventory_start && slot_index < use_row_end {
            if !self.move_item_stack_to(player, &mut stack, 0, result_slot, false) {
                return ItemStack::empty();
            }
        } else if slot_index >= inventory_start && slot_index < inventory_end {
            if !self.move_item_stack_to(player, &mut stack, use_row_start, use_row_end, false) {
                return ItemStack::empty();
            }
        } else if slot_index >= use_row_start && slot_index < use_row_end && !self.move_item_stack_to(player, &mut stack, inventory_start, inventory_end, false) {
            return ItemStack::empty();
        }
        finish_quick_move(self, player, slot_index, stack, &clicked)
    }
}
