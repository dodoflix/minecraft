//! `TransientCraftingContainer`, `ResultContainer`, `ResultSlot` and the crafting grid recompute shared by the menus
//! with a crafting result (`CraftingMenu.slotChangedCraftingGrid`).

use super::{
    ContainerKey, MenuBase, MenuPlayer, Prediction, RecipeCraftingHolder, Slot, SlotBehavior,
    SlotEnv,
};
use crate::world::container::{Container, empty_stack, helper};
use crate::world::item::crafting::stacked_contents::StackedItemContents;
use crate::world::item::crafting::{
    CraftingInput, PositionedCraftingInput, RecipeHolder, default_crafting_reminder,
};
use crate::world::item::{ItemStack, behaviour, is_item_enabled, on_crafted_by};
use crate::world::level::level_access::LevelReader;
use mc_data::RecipeType;
use std::any::Any;
use std::collections::VecDeque;

/// `TransientCraftingContainer` (width x height grid owned by a menu). Every `setItem`/successful `removeItem` owes the
/// menu one `slotsChanged` notification; vanilla calls `menu.slotsChanged(this)` synchronously, so the grid as it was
/// right after that mutation is kept for the menu's replay (`take_snapshot`).
pub struct TransientCraftingContainer {
    items: Vec<ItemStack>,
    width: usize,
    height: usize,
    unannounced: u32,
    snapshots: VecDeque<Vec<ItemStack>>,
}

impl TransientCraftingContainer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            items: vec![ItemStack::empty(); width * height],
            width,
            height,
            unannounced: 0,
            snapshots: VecDeque::new(),
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
    /// `asCraftInput()`.
    pub fn as_craft_input(&self) -> CraftingInput {
        CraftingInput::of(self.width, self.height, &self.items)
    }
    /// `asPositionedCraftInput()`.
    pub fn as_positioned_craft_input(&self) -> PositionedCraftingInput {
        CraftingInput::of_positioned(self.width, self.height, &self.items)
    }
    /// `fillStackedContents(contents)`.
    pub fn fill_stacked_contents(&self, contents: &mut StackedItemContents) {
        for stack in &self.items {
            contents.account_simple_stack(stack);
        }
    }
    /// The grid as it was at the oldest mutation whose `slotsChanged` the menu has not replayed yet (the current grid
    /// when none is queued).
    pub fn take_snapshot(&mut self) -> Vec<ItemStack> {
        self.snapshots
            .pop_front()
            .unwrap_or_else(|| self.items.clone())
    }
    fn changed(&mut self) {
        self.unannounced += 1;
        self.snapshots.push_back(self.items.clone());
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
            self.changed();
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
        self.changed();
    }
    fn clear_content(&mut self) {
        self.items.fill(ItemStack::empty());
    }
    fn set_changed(&mut self) {}
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
    fn take_menu_notifications(&mut self) -> u32 {
        std::mem::take(&mut self.unannounced)
    }
}

/// `ResultContainer` (one slot; every index addresses it).
#[derive(Default)]
pub struct ResultContainer {
    item: ItemStack,
    recipe_used: Option<RecipeHolder>,
}

impl ResultContainer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RecipeCraftingHolder for ResultContainer {
    fn set_recipe_used(&mut self, recipe: Option<RecipeHolder>) {
        self.recipe_used = recipe;
    }
    fn get_recipe_used(&self) -> Option<&RecipeHolder> {
        self.recipe_used.as_ref()
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
    fn set_changed(&mut self) {}
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
}

/// `CraftingMenu.slotChangedCraftingGrid(menu, level, player, craftSlots, resultSlots, recipeHint)` for a grid held in
/// `base.locals[craft_local]` and its `ResultContainer` in `base.locals[result_local]`; `items` is the grid contents to
/// match (the current ones, or the snapshot of the mutation being replayed).
pub fn slot_changed_crafting_grid(
    base: &mut MenuBase,
    player: &mut dyn MenuPlayer,
    craft_local: usize,
    result_local: usize,
    items: &[ItemStack],
    recipe_hint: Option<&RecipeHolder>,
) {
    let craft = base.locals[craft_local]
        .as_any()
        .downcast_ref::<TransientCraftingContainer>()
        .expect("crafting grid");
    let input = CraftingInput::of(craft.get_width(), craft.get_height(), items);
    let results = base.locals[result_local]
        .as_any_mut()
        .downcast_mut::<ResultContainer>()
        .expect("result container");
    let mut result = ItemStack::empty();
    if let Some(level) = player.level_ref() {
        let hint = recipe_hint.map(|recipe| &recipe.id);
        if let Some(recipe) =
            level
                .recipe_access()
                .get_recipe_for(RecipeType::CRAFTING, &input, level, hint)
            && results.set_recipe_used_by(&*player, &recipe)
        {
            let assembled = recipe.value.assemble_crafting(&input);
            if is_item_enabled(&assembled, level.enabled_features()) {
                result = assembled;
            }
        }
    }
    base.locals[result_local].set_item(0, result.clone());
    base.set_remote_slot(0, &result);
    base.send_slot_change_now(0, result);
}

/// `ResultSlot.getRemainingItems(input, level)`; `None` when no recipe matches: vanilla then hands back the grid's own
/// stacks (`copyAllInputItems`), which `on_take` models by reading the live grid slot.
fn remaining_items(player: &mut dyn MenuPlayer, input: &CraftingInput) -> Option<Vec<ItemStack>> {
    let Some(level) = player.level() else {
        return Some(default_crafting_reminder(input));
    };
    level
        .recipe_access()
        .get_recipe_for(RecipeType::CRAFTING, input, level, None)
        .map(|recipe| recipe.value.get_remaining_items(input))
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

    fn check_take_achievements(&self, slot: &Slot, env: &mut SlotEnv, carried: &ItemStack) {
        let count = self.remove_count.get();
        if count > 0 {
            // Not implemented: `Item.onCraftedBy` edits of the taken stack itself (`MapItem` post-processing of a
            // crafted map): `SlotBehavior::on_take` only borrows it, so the post-processed copy is dropped. No vanilla
            // recipe of the 2x2 grid produces such a stack.
            let mut crafted = carried.clone();
            on_crafted_by(&mut crafted, &mut *env.player, count);
        }
        let items = env.locals[self.craft_slots]
            .as_any()
            .downcast_ref::<TransientCraftingContainer>()
            .expect("ResultSlot craft container")
            .get_items()
            .to_vec();
        if let ContainerKey::Local(index) = slot.container
            && let Some(holder) = env.locals[index]
                .as_any_mut()
                .downcast_mut::<ResultContainer>()
        {
            holder.award_used_recipes(&mut *env.player, &items);
        }
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
    fn on_quick_craft(&self, slot: &Slot, env: &mut SlotEnv, picked: &ItemStack, count: i32) {
        self.remove_count.set(self.remove_count.get() + count);
        self.check_take_achievements(slot, env, picked);
    }
    fn on_swap_craft(&self, _slot: &Slot, count: i32) {
        self.remove_count.set(self.remove_count.get() + count);
    }
    fn safe_clone(&self, slot: &Slot, env: &mut SlotEnv) -> ItemStack {
        let mut result = slot.base_safe_clone(env);
        behaviour(result.item()).on_crafted_by(&mut result, &mut *env.player);
        result
    }
    fn on_take(&self, slot: &Slot, env: &mut SlotEnv, carried: &ItemStack) {
        self.check_take_achievements(slot, env, carried);
        let (positioned, craft_width) = {
            let craft = env.locals[self.craft_slots]
                .as_any()
                .downcast_ref::<TransientCraftingContainer>()
                .expect("ResultSlot craft container");
            (craft.as_positioned_craft_input(), craft.get_width())
        };
        let input = &positioned.input;
        let remaining = remaining_items(&mut *env.player, input);
        for y in 0..input.height {
            for x in 0..input.width {
                let grid_slot = x + positioned.left + (y + positioned.top) * craft_width;
                let craft = env.locals[self.craft_slots].as_mut();
                if !craft.get_item(grid_slot).is_empty() {
                    craft.remove_item(grid_slot, 1);
                }
                let mut replacement = match &remaining {
                    Some(remaining) => remaining[x + y * input.width].clone(),
                    None => craft.get_item(grid_slot).clone(),
                };
                if !replacement.is_empty() {
                    let in_slot = craft.get_item(grid_slot).clone();
                    if in_slot.is_empty() {
                        craft.set_item(grid_slot, replacement);
                    } else if ItemStack::is_same_item_same_components(&in_slot, &replacement) {
                        replacement.grow(in_slot.count());
                        craft.set_item(grid_slot, replacement);
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
