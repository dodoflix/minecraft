//! `AbstractFurnaceMenu` with `FurnaceMenu`, `SmokerMenu` and `BlastFurnaceMenu`, `FurnaceFuelSlot` and `FurnaceResultSlot`:
//! input, fuel and result slots over the furnace block entity at `pos` plus its four data slots (burn time, burn duration,
//! cooking progress, cooking time).

use super::{
    AbstractContainerMenu, ContainerKey, MenuBase, MenuPlayer, Slot, SlotBehavior, SlotEnv,
    finish_quick_move, with_player_container,
};
use crate::world::item::crafting::{
    PlaceRecipeMenu, PostPlaceAction, RecipeBookMenu, RecipeHolder, RecipeInput, RecipePropertySet,
    SingleRecipeInput, StackedItemContents, place_recipe,
};
use crate::world::item::{ItemStack, on_crafted_by};
use mc_core::pos::BlockPos;
use mc_data::{Item, MenuType};
use mc_protocol::components::COOKING_FUEL;
use mc_protocol::packets::play::RecipeBookType;
use std::any::Any;
use std::cell::Cell;

pub const INGREDIENT_SLOT: usize = 0;
pub const FUEL_SLOT: usize = 1;
pub const RESULT_SLOT: usize = 2;
pub const SLOT_COUNT: usize = 3;
pub const DATA_COUNT: usize = 4;
const INV_SLOT_START: usize = 3;
const INV_SLOT_END: usize = 30;
const USE_ROW_SLOT_START: usize = 30;
const USE_ROW_SLOT_END: usize = 39;

/// `AbstractFurnaceMenu.isFuel(stack)`.
fn is_fuel(stack: &ItemStack) -> bool {
    stack.get(&COOKING_FUEL).is_some()
}

/// `FurnaceFuelSlot.isBucket(stack)`.
fn is_bucket(stack: &ItemStack) -> bool {
    stack.is(Item::BUCKET)
}

/// `FurnaceFuelSlot`: fuel or a bucket, and a bucket never stacks.
struct FurnaceFuelSlot;

impl SlotBehavior for FurnaceFuelSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        is_fuel(stack) || is_bucket(stack)
    }

    fn get_max_stack_size_for(&self, slot: &Slot, env: &mut SlotEnv, stack: &ItemStack) -> i32 {
        if is_bucket(stack) {
            1
        } else {
            slot.get_max_stack_size(env).min(stack.get_max_stack_size())
        }
    }
}

/// `FurnaceResultSlot`: nothing goes in; taking counts the crafted items and pays out the experience of the recipes used.
#[derive(Default)]
struct FurnaceResultSlot {
    remove_count: Cell<i32>,
}

impl FurnaceResultSlot {
    /// `checkTakeAchievements(carried)`: the crafted statistic, then the furnace awards and forgets its recipes used.
    fn check_take_achievements(&self, slot: &Slot, env: &mut SlotEnv, carried: &mut ItemStack) {
        on_crafted_by(carried, &mut *env.player, self.remove_count.get());
        if env.player.is_server_player()
            && let ContainerKey::BlockEntity(pos) = slot.container
        {
            let position = env.player.position();
            let recipes = env.player.level().and_then(|level| {
                level.with_block_entity(pos, |level, be| {
                    be.award_used_recipes_and_pop_experience(level, position)
                })
            });
            if let Some(recipes) = recipes {
                // `triggerRecipeCrafted(recipe, this.items)`: the furnace's own stacks
                let items: Vec<ItemStack> = env.with_container(&slot.container, |c| {
                    (0..SLOT_COUNT).map(|i| c.get_item(i).clone()).collect()
                });
                env.player.award_recipes(&recipes);
                for recipe in &recipes {
                    env.player.trigger_recipe_crafted(recipe, &items);
                }
            }
        }
        self.remove_count.set(0);
    }
}

impl SlotBehavior for FurnaceResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn remove(&self, slot: &Slot, env: &mut SlotEnv, amount: i32) -> ItemStack {
        if slot.has_item(env) {
            let count = slot.get_item(env).count();
            self.remove_count
                .set(self.remove_count.get() + amount.min(count));
        }
        slot.base_remove(env, amount)
    }

    fn on_take(&self, slot: &Slot, env: &mut SlotEnv, carried: &mut ItemStack) {
        self.check_take_achievements(slot, env, carried);
        slot.set_changed(env);
    }

    fn on_quick_craft(&self, slot: &Slot, env: &mut SlotEnv, picked: &ItemStack, count: i32) {
        self.remove_count.set(self.remove_count.get() + count);
        // `picked` is the quick move's `clicked` copy, which the caller does not read again: a clone is equivalent
        self.check_take_achievements(slot, env, &mut picked.clone());
    }
}

pub struct FurnaceMenu {
    base: MenuBase,
    pos: BlockPos,
    recipe_book_type: RecipeBookType,
    /// `acceptedInputs`: the items the furnace type smelts, taken from the level's recipes when the menu is built.
    accepted_inputs: RecipePropertySet,
}

impl FurnaceMenu {
    /// `new FurnaceMenu(containerId, inventory, container, data)` for the furnace at `pos`.
    pub fn furnace(container_id: i32, pos: BlockPos, player: &dyn MenuPlayer) -> Self {
        Self::new(
            MenuType::FURNACE,
            RecipePropertySet::FURNACE_INPUT,
            RecipeBookType::Furnace,
            container_id,
            pos,
            player,
        )
    }

    /// `new SmokerMenu(containerId, inventory, container, data)` for the smoker at `pos`.
    pub fn smoker(container_id: i32, pos: BlockPos, player: &dyn MenuPlayer) -> Self {
        Self::new(
            MenuType::SMOKER,
            RecipePropertySet::SMOKER_INPUT,
            RecipeBookType::Smoker,
            container_id,
            pos,
            player,
        )
    }

    /// `new BlastFurnaceMenu(containerId, inventory, container, data)` for the blast furnace at `pos`.
    pub fn blast_furnace(container_id: i32, pos: BlockPos, player: &dyn MenuPlayer) -> Self {
        Self::new(
            MenuType::BLAST_FURNACE,
            RecipePropertySet::BLAST_FURNACE_INPUT,
            RecipeBookType::BlastFurnace,
            container_id,
            pos,
            player,
        )
    }

    fn new(
        menu_type: MenuType,
        allowed_inputs: &str,
        recipe_book_type: RecipeBookType,
        container_id: i32,
        pos: BlockPos,
        player: &dyn MenuPlayer,
    ) -> Self {
        let accepted_inputs = player
            .level_ref()
            .map(|level| level.recipe_access().property_set(allowed_inputs).clone())
            .unwrap_or_default();
        let mut base = MenuBase::new(Some(menu_type), container_id);
        let furnace = ContainerKey::BlockEntity(pos);
        base.add_slot(Slot::new(furnace.clone(), INGREDIENT_SLOT, 56, 17));
        base.add_slot(Slot::with_behavior(
            furnace.clone(),
            FUEL_SLOT,
            56,
            53,
            Box::new(FurnaceFuelSlot),
        ));
        base.add_slot(Slot::with_behavior(
            furnace,
            RESULT_SLOT,
            116,
            35,
            Box::new(FurnaceResultSlot::default()),
        ));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        base.add_block_entity_data_slots(pos, DATA_COUNT);
        Self {
            base,
            pos,
            recipe_book_type,
            accepted_inputs,
        }
    }

    /// `canSmelt(stack)`.
    fn can_smelt(&self, stack: &ItemStack) -> bool {
        self.accepted_inputs.test(stack)
    }

    /// `getResultSlot()`.
    pub fn get_result_slot(&self) -> &Slot {
        &self.base.slots[RESULT_SLOT]
    }

    fn data(&self, index: usize) -> i32 {
        self.base.data_slot_values()[index]
    }

    /// `getBurnProgress()`.
    pub fn get_burn_progress(&self) -> f32 {
        let current = self.data(2);
        let total = self.data(3);
        if total != 0 && current != 0 {
            (current as f32 / total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// `getLitProgress()`.
    pub fn get_lit_progress(&self) -> f32 {
        let lit_duration = match self.data(1) {
            0 => 200,
            duration => duration,
        };
        (self.data(0) as f32 / lit_duration as f32).clamp(0.0, 1.0)
    }

    /// `isLit()`.
    pub fn is_lit(&self) -> bool {
        self.data(0) > 0
    }
}

/// The menu side of `ServerPlaceRecipe.CraftingMenuAccess` for a furnace: the input slot is the grid, the input and the
/// result slot are cleared.
struct FurnacePlacement<'a> {
    menu: &'a mut FurnaceMenu,
}

impl FurnacePlacement<'_> {
    const SLOTS_TO_CLEAR: [usize; 2] = [INGREDIENT_SLOT, RESULT_SLOT];

    fn slot_item(&mut self, player: &mut dyn MenuPlayer, slot: usize) -> ItemStack {
        self.menu.base.with_slot(player, slot, |s, e| s.get_item(e))
    }
}

impl PlaceRecipeMenu for FurnacePlacement<'_> {
    fn fill_craft_slots_stacked_contents(
        &mut self,
        player: &mut dyn MenuPlayer,
        contents: &mut StackedItemContents,
    ) {
        self.menu
            .fill_craft_slots_stacked_contents(player, contents);
    }

    fn clear_crafting_content(&mut self, player: &mut dyn MenuPlayer) {
        for slot in Self::SLOTS_TO_CLEAR {
            self.menu
                .base
                .with_slot(player, slot, |s, e| s.set(e, ItemStack::empty()));
        }
    }

    fn recipe_matches(&self, player: &mut dyn MenuPlayer, recipe: &RecipeHolder) -> bool {
        let input = with_player_container(player, &ContainerKey::BlockEntity(self.menu.pos), |c| {
            SingleRecipeInput(c.get_item(INGREDIENT_SLOT).clone())
        });
        player
            .level_ref()
            .is_some_and(|level| input.matches(&recipe.value, level))
    }

    fn input_grid_len(&self) -> usize {
        1
    }

    fn input_grid_item(&mut self, player: &mut dyn MenuPlayer, _index: usize) -> ItemStack {
        self.slot_item(player, INGREDIENT_SLOT)
    }

    fn set_input_grid_item(
        &mut self,
        player: &mut dyn MenuPlayer,
        _index: usize,
        stack: ItemStack,
    ) {
        self.menu
            .base
            .with_slot(player, INGREDIENT_SLOT, |s, e| s.set(e, stack));
    }

    fn grow_input_grid_item(&mut self, player: &mut dyn MenuPlayer, _index: usize, count: i32) {
        self.menu.base.with_slot(player, INGREDIENT_SLOT, |s, e| {
            s.with_item_mut(e, |stack| stack.grow(count))
        });
    }

    fn slots_to_clear_len(&self) -> usize {
        Self::SLOTS_TO_CLEAR.len()
    }

    fn slot_to_clear_item(&mut self, player: &mut dyn MenuPlayer, index: usize) -> ItemStack {
        self.slot_item(player, Self::SLOTS_TO_CLEAR[index])
    }

    fn set_slot_to_clear_item(
        &mut self,
        player: &mut dyn MenuPlayer,
        index: usize,
        stack: ItemStack,
    ) {
        self.menu
            .base
            .with_slot(player, Self::SLOTS_TO_CLEAR[index], |s, e| s.set(e, stack));
    }
}

impl RecipeBookMenu for FurnaceMenu {
    fn handle_placement(
        &mut self,
        player: &mut dyn MenuPlayer,
        use_max_items: bool,
        allow_dropping_items_to_clear: bool,
        recipe: &RecipeHolder,
    ) -> PostPlaceAction {
        place_recipe(
            &mut FurnacePlacement { menu: self },
            player,
            1,
            1,
            recipe,
            use_max_items,
            allow_dropping_items_to_clear,
        )
    }

    /// `container.fillStackedContents(stackedContents)` of the furnace (`accountStack` of its three stacks).
    fn fill_craft_slots_stacked_contents(
        &mut self,
        player: &mut dyn MenuPlayer,
        contents: &mut StackedItemContents,
    ) {
        for slot in 0..SLOT_COUNT {
            let stack = self.base.with_slot(player, slot, |s, e| s.get_item(e));
            contents.account_stack(&stack);
        }
    }

    fn get_recipe_book_type(&self) -> RecipeBookType {
        self.recipe_book_type
    }
}

impl AbstractContainerMenu for FurnaceMenu {
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
    fn as_recipe_book_menu_mut(&mut self) -> Option<&mut dyn RecipeBookMenu> {
        Some(self)
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        ContainerKey::BlockEntity(self.pos).still_valid(player)
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        if !self
            .base
            .with_slot(player, slot_index, |s, e| s.has_item(e))
        {
            return ItemStack::empty();
        }
        let mut stack = self
            .base
            .with_slot(player, slot_index, |s, e| s.get_item(e));
        let clicked = stack.copy();
        if slot_index == RESULT_SLOT {
            if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true)
            {
                return ItemStack::empty();
            }
            // vanilla shrinks the slot's own stack: the furnace's stacks the award reads are already short of what moved
            let rest = stack.clone();
            self.base.with_slot(player, slot_index, |s, e| {
                s.with_item_mut(e, |t| *t = rest);
                s.on_quick_craft(e, &stack, &clicked)
            });
        } else if slot_index != FUEL_SLOT && slot_index != INGREDIENT_SLOT {
            if self.can_smelt(&stack) {
                if !self.move_item_stack_to(
                    player,
                    &mut stack,
                    INGREDIENT_SLOT,
                    INGREDIENT_SLOT + 1,
                    false,
                ) {
                    return ItemStack::empty();
                }
            } else if is_fuel(&stack) {
                if !self.move_item_stack_to(player, &mut stack, FUEL_SLOT, FUEL_SLOT + 1, false) {
                    return ItemStack::empty();
                }
            } else if (INV_SLOT_START..INV_SLOT_END).contains(&slot_index) {
                if !self.move_item_stack_to(
                    player,
                    &mut stack,
                    USE_ROW_SLOT_START,
                    USE_ROW_SLOT_END,
                    false,
                ) {
                    return ItemStack::empty();
                }
            } else if (USE_ROW_SLOT_START..USE_ROW_SLOT_END).contains(&slot_index)
                && !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, INV_SLOT_END, false)
            {
                return ItemStack::empty();
            }
        } else if !self.move_item_stack_to(
            player,
            &mut stack,
            INV_SLOT_START,
            USE_ROW_SLOT_END,
            false,
        ) {
            return ItemStack::empty();
        }
        finish_quick_move(self, player, slot_index, stack, &clicked)
    }
}
