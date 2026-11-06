//! `CraftingContainer`/`TransientCraftingContainer`, `ResultContainer`, `ResultSlot` and `CraftingInput`. No crafting
//! recipe is ever matched: the result slot stays empty, exactly like a server with zero crafting recipes.

use super::{ContainerKey, MenuPlayer, Prediction, Slot, SlotBehavior, SlotEnv};
use crate::world::container::{Container, empty_stack, helper};
use crate::world::item::ItemStack;
use std::any::Any;

/// `CraftingInput` (already trimmed to its bounding box by [`CraftingInput::of_positioned`]).
#[derive(Clone, Debug, Default)]
pub struct CraftingInput {
    pub width: usize,
    pub height: usize,
    pub items: Vec<ItemStack>,
}

/// `CraftingInput.Positioned`.
#[derive(Clone, Debug, Default)]
pub struct PositionedCraftingInput {
    pub input: CraftingInput,
    pub left: usize,
    pub top: usize,
}

impl CraftingInput {
    pub fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
    }

    pub fn of_positioned(
        width: usize,
        height: usize,
        items: &[ItemStack],
    ) -> PositionedCraftingInput {
        if width == 0 || height == 0 {
            return PositionedCraftingInput::default();
        }
        let (mut left, mut right, mut top, mut bottom) = (width - 1, 0usize, height - 1, 0usize);
        for y in 0..height {
            let mut row_empty = true;
            for x in 0..width {
                if !items[x + y * width].is_empty() {
                    left = left.min(x);
                    right = right.max(x);
                    row_empty = false;
                }
            }
            if !row_empty {
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
        if right < left || bottom < top {
            return PositionedCraftingInput::default();
        }
        let (nw, nh) = (right - left + 1, bottom - top + 1);
        let new_items = (0..nh)
            .flat_map(|y| (0..nw).map(move |x| (x, y)))
            .map(|(x, y)| items[x + left + (y + top) * width].clone())
            .collect();
        PositionedCraftingInput {
            input: CraftingInput {
                width: nw,
                height: nh,
                items: new_items,
            },
            left,
            top,
        }
    }
}

/// `TransientCraftingContainer` (width x height grid owned by a menu). Every `setItem`/successful `removeItem` owes the
/// menu one `slotsChanged` notification (vanilla calls `menu.slotsChanged(this)` synchronously).
pub struct TransientCraftingContainer {
    items: Vec<ItemStack>,
    width: usize,
    height: usize,
    pending: u32,
}

impl TransientCraftingContainer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            items: vec![ItemStack::empty(); width * height],
            width,
            height,
            pending: 0,
        }
    }
    pub fn get_width(&self) -> usize {
        self.width
    }
    pub fn get_height(&self) -> usize {
        self.height
    }
    pub fn get_items(&self) -> &[ItemStack] {
        &self.items
    }
    /// `asPositionedCraftInput()`.
    pub fn as_positioned_craft_input(&self) -> PositionedCraftingInput {
        CraftingInput::of_positioned(self.width, self.height, &self.items)
    }
}

impl Container for TransientCraftingContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.items.len()
    }
    fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.items.get(slot).unwrap_or_else(|| empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.items.get_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        let result = helper::remove_item(&mut self.items, slot, count);
        if !result.is_empty() {
            self.pending += 1;
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        helper::take_item(&mut self.items, slot)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if let Some(s) = self.items.get_mut(slot) {
            *s = stack;
        }
        self.pending += 1;
    }
    fn clear_content(&mut self) {
        self.items.fill(ItemStack::empty());
    }
    fn set_changed(&mut self) {}
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
    fn take_menu_notifications(&mut self) -> u32 {
        std::mem::take(&mut self.pending)
    }
}

/// `ResultContainer` (one slot; the `recipeUsed` bookkeeping is not implemented).
#[derive(Default)]
pub struct ResultContainer {
    item: ItemStack,
}

impl ResultContainer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Container for ResultContainer {
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
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        (slot == 0).then_some(&mut self.item)
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
    fn set_changed(&mut self) {}
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
}

/// `ResultSlot` behavior; `craft_slots` is the menu-local index of the `TransientCraftingContainer`.
pub struct ResultSlot {
    craft_slots: usize,
    remove_count: std::cell::Cell<i32>,
}

impl ResultSlot {
    pub fn new(craft_slots: usize) -> Self {
        Self {
            craft_slots,
            remove_count: std::cell::Cell::new(0),
        }
    }

    fn check_take_achievements(&self) {
        // `carried.onCraftedBy(player, removeCount)` (item behaviour `on_crafted`, crafting stats/advancements) and
        // `awardUsedRecipes`: not implemented.
        self.remove_count.set(0);
    }
}

impl SlotBehavior for ResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }
    fn remove(&self, slot: &Slot, env: &mut SlotEnv, amount: i32) -> ItemStack {
        if slot.get_item(env).count() > 0 {
            self.remove_count
                .set(self.remove_count.get() + amount.min(slot.get_item(env).count()));
        }
        slot.base_remove(env, amount)
    }
    fn on_quick_craft(&self, _slot: &Slot, _env: &mut SlotEnv, _picked: &ItemStack, count: i32) {
        self.remove_count.set(self.remove_count.get() + count);
        self.check_take_achievements();
    }
    fn on_swap_craft(&self, _slot: &Slot, count: i32) {
        self.remove_count.set(self.remove_count.get() + count);
    }
    fn safe_clone(&self, slot: &Slot, env: &mut SlotEnv) -> ItemStack {
        // `result.getItem().onCraftedBy(result, player)`: item behaviour hook (no-op for every vanilla item but maps/books).
        slot.base_safe_clone(env)
    }
    fn on_take(&self, _slot: &Slot, env: &mut SlotEnv, _carried: &ItemStack) {
        self.check_take_achievements();
        let (positioned, craft_width) = {
            let craft = env.locals[self.craft_slots]
                .as_any()
                .downcast_ref::<TransientCraftingContainer>()
                .expect("ResultSlot craft container");
            (craft.as_positioned_craft_input(), craft.get_width())
        };
        let input = &positioned.input;
        let mut remaining = input.items.clone();
        for y in 0..input.height {
            for x in 0..input.width {
                let slot = x + positioned.left + (y + positioned.top) * craft_width;
                let replacement = std::mem::take(&mut remaining[x + y * input.width]);
                let craft = env.locals[self.craft_slots].as_mut();
                if !craft.get_item(slot).is_empty() {
                    craft.remove_item(slot, 1);
                }
                let mut replacement = replacement;
                if !replacement.is_empty() {
                    let in_slot = craft.get_item(slot).clone();
                    if in_slot.is_empty() {
                        craft.set_item(slot, replacement);
                    } else if ItemStack::is_same_item_same_components(&in_slot, &replacement) {
                        replacement.grow(in_slot.count());
                        craft.set_item(slot, replacement);
                    } else {
                        let infinite = env.player.has_infinite_materials();
                        if !env.player.inventory_mut().add(&mut replacement, infinite) {
                            env.player.drop(replacement, false, Prediction::Predicted);
                        }
                    }
                }
            }
        }
    }
    fn is_fake(&self) -> bool {
        true
    }
}

/// `AbstractCraftingMenu.addResultSlot`.
pub fn result_slot(result_local: usize, craft_local: usize, x: i32, y: i32) -> Slot {
    Slot::with_behavior(
        ContainerKey::Local(result_local),
        0,
        x,
        y,
        Box::new(ResultSlot::new(craft_local)),
    )
}
