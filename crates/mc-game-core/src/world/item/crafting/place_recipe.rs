//! The placement algorithm of `RecipeBookMenu`: `ServerPlaceRecipe` (the "click a recipe in the book" algorithm that moves
//! the needed items from the inventory into the crafting grid) and `PlaceRecipeHelper`.
//!
//! Menus never hold the player in this port, so the menu side of `ServerPlaceRecipe.CraftingMenuAccess` plus the slot lists is
//! the [`PlaceRecipeMenu`] trait and the inventory is reached through the [`MenuPlayer`]. Menus with a recipe book implement
//! [`RecipeBookMenu`]; the crafting grids share [`handle_crafting_placement`] (`AbstractCraftingMenu.handlePlacement`).

use std::ops::Range;

use mc_data::Item;
use mc_protocol::packets::play::RecipeBookType;

use super::recipe::{Recipe, RecipeKind};
use super::recipe_manager::RecipeHolder;
use super::stacked_contents::StackedItemContents;
use crate::world::container::Container;
use crate::world::inventory::{
    AbstractContainerMenu, MenuPlayer, Prediction, TransientCraftingContainer,
    place_item_back_in_inventory,
};
use crate::world::item::ItemStack;

/// `RecipeBookMenu.PostPlaceAction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PostPlaceAction {
    Nothing,
    PlaceGhostRecipe,
}

/// `RecipeBookMenu`: a menu the recipe book can fill (`AbstractContainerMenu.as_recipe_book_menu_mut`).
pub trait RecipeBookMenu: AbstractContainerMenu {
    /// `handlePlacement(useMaxItems, allowDroppingItemsToClear, recipe, level, inventory)`; the level and the inventory
    /// are the player's.
    fn handle_placement(
        &mut self,
        player: &mut dyn MenuPlayer,
        use_max_items: bool,
        allow_dropping_items_to_clear: bool,
        recipe: &RecipeHolder,
    ) -> PostPlaceAction;
    /// `fillCraftSlotsStackedContents(stackedContents)`; the player reaches the containers that live in the level.
    fn fill_craft_slots_stacked_contents(
        &mut self,
        player: &mut dyn MenuPlayer,
        contents: &mut StackedItemContents,
    );
    /// `getRecipeBookType()`.
    fn get_recipe_book_type(&self) -> RecipeBookType;
}

/// `ServerPlaceRecipe.CraftingMenuAccess` + `Slot` access of the input grid / slots to clear.
pub trait PlaceRecipeMenu {
    /// `fillCraftSlotsStackedContents(stackedContents)`.
    fn fill_craft_slots_stacked_contents(
        &mut self,
        player: &mut dyn MenuPlayer,
        contents: &mut StackedItemContents,
    );
    /// `clearCraftingContent()`.
    fn clear_crafting_content(&mut self, player: &mut dyn MenuPlayer);
    /// `recipeMatches(recipe)`; `None` when the recipe is not of the menu's recipe class: vanilla's unchecked cast of the
    /// holder throws a `ClassCastException` there, which the packet processor suppresses (nothing else happens).
    fn recipe_matches(&self, player: &mut dyn MenuPlayer, recipe: &RecipeHolder) -> Option<bool>;

    /// `inputGridSlots.size()`.
    fn input_grid_len(&self) -> usize;
    /// `inputGridSlots.get(i).getItem()`.
    fn input_grid_item(&mut self, player: &mut dyn MenuPlayer, index: usize) -> ItemStack;
    /// `inputGridSlots.get(i).set(stack)`.
    fn set_input_grid_item(&mut self, player: &mut dyn MenuPlayer, index: usize, stack: ItemStack);
    /// `inputGridSlots.get(i).getItem().grow(count)` (in place, no change notification).
    fn grow_input_grid_item(&mut self, player: &mut dyn MenuPlayer, index: usize, count: i32);

    /// `slotsToClear.size()`.
    fn slots_to_clear_len(&self) -> usize;
    /// `slotsToClear.get(i).getItem()`.
    fn slot_to_clear_item(&mut self, player: &mut dyn MenuPlayer, index: usize) -> ItemStack;
    /// `slotsToClear.get(i).set(stack)`.
    fn set_slot_to_clear_item(
        &mut self,
        player: &mut dyn MenuPlayer,
        index: usize,
        stack: ItemStack,
    );
}

/// The crafting grid of an `AbstractCraftingMenu`: the menu-local grid and result containers and the slots (in order,
/// `getInputGridSlots()`) that show the grid.
#[derive(Clone, Debug)]
pub struct CraftingGrid {
    pub craft_local: usize,
    pub result_local: usize,
    pub input_slots: Range<usize>,
    pub width: i32,
    pub height: i32,
}

/// `AbstractCraftingMenu.handlePlacement` for the menu `menu` (without `begin/finishPlacingRecipe`, which a menu
/// overriding them runs around this call). The `slotsChanged` of every grid change runs as the change is made.
pub fn handle_crafting_placement(
    menu: &mut dyn AbstractContainerMenu,
    grid: &CraftingGrid,
    player: &mut dyn MenuPlayer,
    use_max_items: bool,
    allow_dropping_items_to_clear: bool,
    recipe: &RecipeHolder,
) -> PostPlaceAction {
    place_recipe(
        &mut CraftingGridAccess { menu, grid },
        player,
        grid.width,
        grid.height,
        recipe,
        use_max_items,
        allow_dropping_items_to_clear,
    )
}

struct CraftingGridAccess<'a> {
    menu: &'a mut dyn AbstractContainerMenu,
    grid: &'a CraftingGrid,
}

impl CraftingGridAccess<'_> {
    fn craft_slots(&self) -> &TransientCraftingContainer {
        self.menu.base().locals[self.grid.craft_local]
            .as_any()
            .downcast_ref()
            .expect("crafting grid")
    }
}

impl PlaceRecipeMenu for CraftingGridAccess<'_> {
    fn fill_craft_slots_stacked_contents(
        &mut self,
        _player: &mut dyn MenuPlayer,
        contents: &mut StackedItemContents,
    ) {
        self.craft_slots().fill_stacked_contents(contents);
    }
    fn clear_crafting_content(&mut self, _player: &mut dyn MenuPlayer) {
        let base = self.menu.base_mut();
        base.locals[self.grid.result_local].clear_content();
        base.locals[self.grid.craft_local].clear_content();
    }
    fn recipe_matches(&self, player: &mut dyn MenuPlayer, recipe: &RecipeHolder) -> Option<bool> {
        recipe.value.is_crafting_recipe().then(|| {
            player.level_ref().is_some_and(|level| {
                recipe
                    .value
                    .matches_crafting(&self.craft_slots().as_craft_input(), level)
            })
        })
    }

    fn input_grid_len(&self) -> usize {
        self.grid.input_slots.len()
    }
    fn input_grid_item(&mut self, player: &mut dyn MenuPlayer, index: usize) -> ItemStack {
        let slot = self.grid.input_slots.start + index;
        self.menu
            .base_mut()
            .with_slot(player, slot, |s, e| s.get_item(e))
    }
    fn set_input_grid_item(&mut self, player: &mut dyn MenuPlayer, index: usize, stack: ItemStack) {
        let slot = self.grid.input_slots.start + index;
        self.menu
            .base_mut()
            .with_slot(player, slot, |s, e| s.set(e, stack));
        // the grid's `slotsChanged` runs synchronously inside `Slot.set`
        self.menu.flush_container_events(player);
    }
    fn grow_input_grid_item(&mut self, player: &mut dyn MenuPlayer, index: usize, count: i32) {
        let slot = self.grid.input_slots.start + index;
        self.menu.base_mut().with_slot(player, slot, |s, e| {
            s.with_item_mut(e, |stack| stack.grow(count))
        });
    }

    fn slots_to_clear_len(&self) -> usize {
        self.input_grid_len()
    }
    fn slot_to_clear_item(&mut self, player: &mut dyn MenuPlayer, index: usize) -> ItemStack {
        self.input_grid_item(player, index)
    }
    fn set_slot_to_clear_item(
        &mut self,
        player: &mut dyn MenuPlayer,
        index: usize,
        stack: ItemStack,
    ) {
        self.set_input_grid_item(player, index, stack);
    }
}

/// `PlaceRecipeHelper.placeRecipe(gridWidth, gridHeight, recipe, entries, output)`: `output(entry, gridIndex, x, y)`.
pub fn place_recipe_helper<T: Copy>(
    grid_width: i32,
    grid_height: i32,
    recipe: &Recipe,
    entries: &[T],
    output: &mut dyn FnMut(T, i32, i32, i32),
) {
    match &recipe.kind {
        RecipeKind::Shaped(shaped) => place_recipe_sized(
            grid_width,
            grid_height,
            shaped.width() as i32,
            shaped.height() as i32,
            entries,
            output,
        ),
        _ => place_recipe_sized(
            grid_width,
            grid_height,
            grid_width,
            grid_height,
            entries,
            output,
        ),
    }
}

/// `PlaceRecipeHelper.placeRecipe(gridWidth, gridHeight, recipeWidth, recipeHeight, entries, output)`.
pub fn place_recipe_sized<T: Copy>(
    grid_width: i32,
    grid_height: i32,
    recipe_width: i32,
    recipe_height: i32,
    entries: &[T],
    output: &mut dyn FnMut(T, i32, i32, i32),
) {
    let mut iterator = entries.iter();
    let mut grid_index = 0;
    let mut grid_y_pos = 0;
    while grid_y_pos < grid_height {
        let mut should_center_recipe = (recipe_height as f32) < grid_height as f32 / 2.0;
        let mut start_pos_center_recipe =
            (grid_height as f32 / 2.0 - recipe_height as f32 / 2.0).floor() as i32;
        if should_center_recipe && start_pos_center_recipe > grid_y_pos {
            grid_index += grid_width;
            grid_y_pos += 1;
        }
        for grid_x_pos in 0..grid_width {
            let Some(entry) = iterator.clone().next() else {
                return;
            };
            should_center_recipe = (recipe_width as f32) < grid_width as f32 / 2.0;
            start_pos_center_recipe =
                (grid_width as f32 / 2.0 - recipe_width as f32 / 2.0).floor() as i32;
            let mut total_recipe_width_in_grid = recipe_width;
            let mut add_ingredient_to_slot = grid_x_pos < recipe_width;
            if should_center_recipe {
                total_recipe_width_in_grid = start_pos_center_recipe + recipe_width;
                add_ingredient_to_slot = start_pos_center_recipe <= grid_x_pos
                    && grid_x_pos < start_pos_center_recipe + recipe_width;
            }
            if add_ingredient_to_slot {
                iterator.next();
                output(*entry, grid_index, grid_x_pos, grid_y_pos);
            } else if total_recipe_width_in_grid == grid_x_pos {
                grid_index += grid_width - grid_x_pos;
                break;
            }
            grid_index += 1;
        }
        grid_y_pos += 1;
    }
}

/// `ServerPlaceRecipe.placeRecipe(menu, gridWidth, gridHeight, inputGridSlots, slotsToClear, inventory, recipe, useMaxItems,
/// allowDroppingItemsToClear)`.
pub fn place_recipe(
    menu: &mut dyn PlaceRecipeMenu,
    player: &mut dyn MenuPlayer,
    grid_width: i32,
    grid_height: i32,
    recipe: &RecipeHolder,
    use_max_items: bool,
    allow_dropping_items_to_clear: bool,
) -> PostPlaceAction {
    let mut placer = ServerPlaceRecipe {
        menu,
        player,
        use_max_items,
        grid_width,
        grid_height,
    };
    if !allow_dropping_items_to_clear && !placer.test_clear_grid() {
        return PostPlaceAction::Nothing;
    }
    let mut available_items = StackedItemContents::new();
    for stack in placer.player.inventory().get_non_equipment_items() {
        available_items.account_simple_stack(stack);
    }
    placer
        .menu
        .fill_craft_slots_stacked_contents(&mut *placer.player, &mut available_items);
    placer.try_place_recipe(recipe, &mut available_items)
}

struct ServerPlaceRecipe<'a> {
    menu: &'a mut dyn PlaceRecipeMenu,
    player: &'a mut dyn MenuPlayer,
    use_max_items: bool,
    grid_width: i32,
    grid_height: i32,
}

impl ServerPlaceRecipe<'_> {
    fn try_place_recipe(
        &mut self,
        recipe: &RecipeHolder,
        available_items: &mut StackedItemContents,
    ) -> PostPlaceAction {
        if available_items.can_craft(&recipe.value, None) {
            self.place_recipe(recipe, available_items);
            self.player.inventory_mut().set_changed();
            PostPlaceAction::Nothing
        } else {
            self.clear_grid();
            self.player.inventory_mut().set_changed();
            PostPlaceAction::PlaceGhostRecipe
        }
    }

    fn clear_grid(&mut self) {
        for index in 0..self.menu.slots_to_clear_len() {
            let mut stack_copy = self.menu.slot_to_clear_item(self.player, index).copy();
            place_item_back_in_inventory(
                self.player,
                &mut stack_copy,
                false,
                Prediction::ServerOnly,
            );
            self.menu
                .set_slot_to_clear_item(self.player, index, stack_copy);
        }
        self.menu.clear_crafting_content(self.player);
    }

    fn place_recipe(&mut self, recipe: &RecipeHolder, available_items: &mut StackedItemContents) {
        let Some(recipe_matches_placed) = self.menu.recipe_matches(self.player, recipe) else {
            return;
        };
        let biggest_craftable_stack =
            available_items.get_biggest_craftable_stack(&recipe.value, i32::MAX, None);
        if recipe_matches_placed {
            for index in 0..self.menu.input_grid_len() {
                let stack = self.menu.input_grid_item(self.player, index);
                if !stack.is_empty()
                    && biggest_craftable_stack.min(stack.get_max_stack_size()) < stack.count() + 1
                {
                    return;
                }
            }
        }
        let amount_to_craft =
            self.calculate_amount_to_craft(biggest_craftable_stack, recipe_matches_placed);
        let mut items_used_per_ingredient: Vec<Item> = Vec::new();
        let mut collect = |item: Item| items_used_per_ingredient.push(item);
        if !available_items.can_craft_amount(&recipe.value, amount_to_craft, Some(&mut collect)) {
            return;
        }
        let adjusted_amount_to_craft =
            clamp_to_max_stack_size(amount_to_craft, &items_used_per_ingredient);
        if adjusted_amount_to_craft != amount_to_craft {
            items_used_per_ingredient.clear();
            let mut collect = |item: Item| items_used_per_ingredient.push(item);
            if !available_items.can_craft_amount(
                &recipe.value,
                adjusted_amount_to_craft,
                Some(&mut collect),
            ) {
                return;
            }
        }
        self.clear_grid();
        let slots: Vec<i32> = recipe
            .value
            .placement_info()
            .slots_to_ingredient_index()
            .to_vec();
        let (grid_width, grid_height) = (self.grid_width, self.grid_height);
        let items = &items_used_per_ingredient;
        place_recipe_helper(
            grid_width,
            grid_height,
            &recipe.value,
            &slots,
            &mut |ingredient_index, grid_index, _x, _y| {
                if ingredient_index != -1 {
                    let item_used = items[ingredient_index as usize];
                    let mut remaining_count = adjusted_amount_to_craft;
                    while remaining_count > 0 {
                        remaining_count =
                            self.move_item_to_grid(grid_index as usize, item_used, remaining_count);
                        if remaining_count == -1 {
                            return;
                        }
                    }
                }
            },
        );
    }

    fn calculate_amount_to_craft(
        &mut self,
        biggest_craftable_stack: i32,
        recipe_matches_placed: bool,
    ) -> i32 {
        if self.use_max_items {
            biggest_craftable_stack
        } else if recipe_matches_placed {
            let mut smallest_stack_size = i32::MAX;
            for index in 0..self.menu.input_grid_len() {
                let stack = self.menu.input_grid_item(self.player, index);
                if !stack.is_empty() && smallest_stack_size > stack.count() {
                    smallest_stack_size = stack.count();
                }
            }
            smallest_stack_size = smallest_stack_size.saturating_add(1);
            smallest_stack_size
        } else {
            1
        }
    }

    fn move_item_to_grid(
        &mut self,
        target_slot: usize,
        item_in_inventory: Item,
        count: i32,
    ) -> i32 {
        let item_in_target_slot = self.menu.input_grid_item(self.player, target_slot);
        let inventory_slot_id = self
            .player
            .inventory()
            .find_slot_matching_crafting_ingredient(item_in_inventory, &item_in_target_slot);
        if inventory_slot_id == -1 {
            return -1;
        }
        let slot = inventory_slot_id as usize;
        let inventory_count = Container::get_item(self.player.inventory(), slot).count();
        let taken_stack = if count < inventory_count {
            Container::remove_item(self.player.inventory_mut(), slot, count)
        } else {
            Container::remove_item_no_update(self.player.inventory_mut(), slot)
        };
        let taken_count = taken_stack.count();
        if item_in_target_slot.is_empty() {
            self.menu
                .set_input_grid_item(self.player, target_slot, taken_stack);
        } else {
            self.menu
                .grow_input_grid_item(self.player, target_slot, taken_count);
        }
        count - taken_count
    }

    fn test_clear_grid(&mut self) -> bool {
        let mut free_slots: Vec<ItemStack> = Vec::new();
        let free_slots_in_inventory = self
            .player
            .inventory()
            .get_non_equipment_items()
            .iter()
            .filter(|i| i.is_empty())
            .count();
        for index in 0..self.menu.input_grid_len() {
            let mut item_stack = self.menu.input_grid_item(self.player, index).copy();
            if item_stack.is_empty() {
                continue;
            }
            let slot_id = self
                .player
                .inventory()
                .get_slot_with_remaining_space(&item_stack);
            if slot_id == -1 && free_slots.len() <= free_slots_in_inventory {
                for in_list in &mut free_slots {
                    if ItemStack::is_same_item(in_list, &item_stack)
                        && in_list.count() != in_list.get_max_stack_size()
                        && in_list.count() + item_stack.count() <= in_list.get_max_stack_size()
                    {
                        in_list.grow(item_stack.count());
                        item_stack.set_count(0);
                        break;
                    }
                }
                if !item_stack.is_empty() {
                    if free_slots.len() >= free_slots_in_inventory {
                        return false;
                    }
                    free_slots.push(item_stack);
                }
            } else if slot_id == -1 {
                return false;
            }
        }
        true
    }
}

/// `clampToMaxStackSize(value, items)`.
fn clamp_to_max_stack_size(mut value: i32, items: &[Item]) -> i32 {
    for item in items {
        value = value.min(i32::from(item.data().max_stack_size));
    }
    value
}
