//! `StonecutterMenu`: the recipes that accept the input item, the selected one's result and the sound of taking it.
//!
//! Vanilla's input `SimpleContainer` calls `slotsChanged` from `setChanged()`, which re-lists the recipes on the spot (and
//! `ResultSlot.onTake` depends on that: removing the last input item resets the selection before the result is rebuilt).
//! The state that recompute touches therefore lives in [`State`], shared by the input container, the result slot and the
//! menu; the one thing it cannot do itself is `broadcastChanges()` (and the sound that follows it in `onTake`), which it asks
//! for (`broadcast_requested`, `sound_requested`) and the menu replays right after the slot operation.
//!
//! Not implemented: the recipes are those of the level when the menu opened, so stonecutter recipes changed by `/reload` while
//! it is open are only seen once it is reopened.

use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, DataSlot, MenuBase, MenuPlayer,
    MenuSimpleContainer, Prediction, RecipeCraftingHolder, ResultContainer, SharedContainer, Slot,
    SlotBehavior, SlotEnv, finish_quick_move,
};
use crate::world::container::Container;
use crate::world::item::crafting::{RecipeManager, SingleInputSet};
use crate::world::item::{ItemStack, behaviour, on_crafted_by};
use mc_data::{Block, MenuType, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

pub const INPUT_SLOT: usize = 0;
pub const RESULT_SLOT: usize = 1;
const INV_SLOT_START: usize = 2;
const INV_SLOT_END: usize = 29;
const USE_ROW_SLOT_START: usize = 29;
const USE_ROW_SLOT_END: usize = 38;

/// Menu-local container: the input.
const INPUT_LOCAL: usize = 0;

/// The fields of `StonecutterMenu` that `slotsChanged`, `setupResultSlot` and the result slot's `onTake` read and write.
struct State {
    /// `level.recipeAccess()`.
    recipes: RecipeManager,
    /// `selectedRecipeIndex` (the data slot's storage).
    selected: Rc<RefCell<Vec<i32>>>,
    result: Rc<RefCell<ResultContainer>>,
    /// `recipesForInput`.
    recipes_for_input: SingleInputSet,
    /// `input`: a copy of the stack the recipes were listed for.
    input: ItemStack,
    last_sound_time: i64,
    broadcast_requested: bool,
    sound_requested: bool,
}

type Shared = Rc<RefCell<State>>;

impl State {
    fn selected_recipe_index(&self) -> i32 {
        self.selected.borrow()[0]
    }

    fn set_selected_recipe_index(&self, index: i32) {
        self.selected.borrow_mut()[0] = index;
    }

    /// `isValidRecipeIndex(buttonId)`.
    fn is_valid_recipe_index(&self, button_id: i32) -> bool {
        button_id >= 0 && (button_id as usize) < self.recipes_for_input.size()
    }

    /// `slotsChanged(container)`: re-lists the recipes when the input item changed (not for a count change).
    fn slots_changed(&mut self, input: &ItemStack) {
        if !input.is(self.input.item()) {
            self.input = input.copy();
            self.setup_recipe_list(input);
        }
    }

    /// `setupRecipeList(item)`.
    fn setup_recipe_list(&mut self, item: &ItemStack) {
        self.set_selected_recipe_index(-1);
        self.result.borrow_mut().set_item(0, ItemStack::empty());
        self.recipes_for_input = if item.is_empty() {
            SingleInputSet::default()
        } else {
            self.recipes.stonecutter_recipes().select_by_input(item)
        };
    }

    /// `setupResultSlot(index)` up to its `broadcastChanges()`.
    fn setup_result_slot(&mut self, index: i32) {
        let used_recipe = if !self.recipes_for_input.is_empty() && self.is_valid_recipe_index(index)
        {
            self.recipes_for_input.entries[index as usize]
                .recipe
                .recipe
                .clone()
        } else {
            None
        };
        let mut result = self.result.borrow_mut();
        match used_recipe {
            Some(recipe) => {
                let assembled = recipe
                    .value
                    .as_stonecutter()
                    .expect("a stonecutter entry holds a stonecutter recipe")
                    .result
                    .create();
                result.set_recipe_used(Some(recipe));
                result.set_item(0, assembled);
            }
            None => {
                result.set_item(0, ItemStack::empty());
                result.set_recipe_used(None);
            }
        }
        self.broadcast_requested = true;
    }
}

/// `StonecutterMenu.container`: a one-slot `SimpleContainer` whose `setChanged` runs `slotsChanged`.
struct StonecutterInput {
    inner: MenuSimpleContainer,
    state: Shared,
}

impl StonecutterInput {
    fn changed(&mut self) {
        let input = self.inner.get_item(0).clone();
        self.state.borrow_mut().slots_changed(&input);
    }
}

impl Container for StonecutterInput {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.inner.get_container_size()
    }
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.inner.get_item(slot)
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.inner.get_item_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        let result = self.inner.remove_item(slot, count);
        if !result.is_empty() {
            self.changed();
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.inner.remove_item_no_update(slot)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        self.inner.set_item(slot, stack);
        self.changed();
    }
    fn clear_content(&mut self) {
        self.inner.clear_content();
        self.changed();
    }
    fn set_changed(&mut self) {
        self.inner.set_changed();
        self.changed();
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.inner.still_valid(player)
    }
    fn get_max_stack_size(&self) -> i32 {
        self.inner.get_max_stack_size()
    }
    fn take_menu_notifications(&mut self) -> u32 {
        self.inner.take_menu_notifications()
    }
}

/// The result slot: nothing can be placed, taking consumes one input item and re-lists the result.
struct StonecutterResultSlot {
    state: Shared,
}

impl SlotBehavior for StonecutterResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }

    fn on_take(&self, slot: &Slot, env: &mut SlotEnv, carried: &mut ItemStack) {
        let count = carried.count();
        on_crafted_by(carried, &mut *env.player, count);
        let result = self.state.borrow().result.clone();
        let input = env.locals[INPUT_LOCAL].get_item(0).clone();
        result
            .borrow_mut()
            .award_used_recipes(&mut *env.player, std::slice::from_ref(&input));
        let remaining = env.locals[INPUT_LOCAL].remove_item(0, 1);
        if !remaining.is_empty() {
            let mut state = self.state.borrow_mut();
            let selected = state.selected_recipe_index();
            state.setup_result_slot(selected);
        }
        // the broadcast and the sound are replayed by the menu in vanilla's order (`slots_changed`)
        self.state.borrow_mut().sound_requested = true;
        env.locals[INPUT_LOCAL].set_changed();
        slot.set_changed(env);
    }
}

pub struct StonecutterMenu {
    base: MenuBase,
    access: ContainerLevelAccess,
    state: Shared,
    /// `resultContainer`.
    result_key: ContainerKey,
}

impl StonecutterMenu {
    /// `StonecutterMenu(containerId, inventory, access)`.
    pub fn new(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        access: ContainerLevelAccess,
    ) -> Self {
        let mut base = MenuBase::new(Some(MenuType::STONECUTTER), container_id);
        let selected = Rc::new(RefCell::new(vec![0]));
        let result = Rc::new(RefCell::new(ResultContainer::new()));
        let state = Rc::new(RefCell::new(State {
            recipes: player
                .level_ref()
                .map(|level| level.recipe_access())
                .unwrap_or_default(),
            selected: selected.clone(),
            result: result.clone(),
            recipes_for_input: SingleInputSet::default(),
            input: ItemStack::empty(),
            last_sound_time: 0,
            broadcast_requested: false,
            sound_requested: false,
        }));
        let input = base.add_local(Box::new(StonecutterInput {
            inner: MenuSimpleContainer::new(1),
            state: state.clone(),
        }));
        debug_assert_eq!(input, INPUT_LOCAL);
        let result_key = ContainerKey::Shared(result as SharedContainer);
        base.add_slot(Slot::new(ContainerKey::Local(INPUT_LOCAL), 0, 20, 33));
        // the vanilla slot index into `resultContainer` is 1; the container has one slot and ignores it
        base.add_slot(Slot::with_behavior(
            result_key.clone(),
            0,
            143,
            33,
            Box::new(StonecutterResultSlot {
                state: state.clone(),
            }),
        ));
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        base.add_data_slot(DataSlot::shared(selected, 0));
        Self {
            base,
            access,
            state,
            result_key,
        }
    }

    /// `getSelectedRecipeIndex()`.
    pub fn get_selected_recipe_index(&self) -> i32 {
        self.state.borrow().selected_recipe_index()
    }

    /// `getNumberOfVisibleRecipes()`.
    pub fn get_number_of_visible_recipes(&self) -> usize {
        self.state.borrow().recipes_for_input.size()
    }

    /// `setupResultSlot(index)`.
    fn setup_result_slot(&mut self, player: &mut dyn MenuPlayer, index: i32) {
        let mut state = self.state.borrow_mut();
        state.setup_result_slot(index);
        state.broadcast_requested = false;
        drop(state);
        self.broadcast_changes(player);
    }
}

impl AbstractContainerMenu for StonecutterMenu {
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

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        super::still_valid(&self.access, player, Block::STONECUTTER)
    }

    fn click_menu_button(&mut self, player: &mut dyn MenuPlayer, button_id: i32) -> bool {
        if self.get_selected_recipe_index() == button_id {
            return false;
        }
        if self.state.borrow().is_valid_recipe_index(button_id) {
            self.state.borrow().set_selected_recipe_index(button_id);
            self.setup_result_slot(player, button_id);
        }
        true
    }

    /// Vanilla's `slotsChanged` only re-lists the recipes, which the input container already did when it changed; what is
    /// left is what the result slot's `onTake` does after `setupResultSlot`: its `broadcastChanges()`, then the sound.
    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, _container: &ContainerKey) {
        let (broadcast, sound) = {
            let mut state = self.state.borrow_mut();
            (
                std::mem::take(&mut state.broadcast_requested),
                std::mem::take(&mut state.sound_requested),
            )
        };
        if broadcast {
            self.broadcast_changes(player);
        }
        if sound
            && let Some(pos) = self.access.pos()
            && let Some(level) = player.level()
        {
            let game_time = level.game_time();
            let mut state = self.state.borrow_mut();
            if state.last_sound_time != game_time {
                level.play_sound(
                    None,
                    pos,
                    SoundEvent::UI_STONECUTTER_TAKE_RESULT,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                state.last_sound_time = game_time;
            }
        }
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, target: &Slot) -> bool {
        !target.container.same_container(&self.result_key)
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
            behaviour(stack.item()).on_crafted_by(&mut stack, player);
            if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true)
            {
                return ItemStack::empty();
            }
            self.base.with_slot(player, slot_index, |s, e| {
                s.on_quick_craft(e, &stack, &clicked)
            });
        } else if slot_index == INPUT_SLOT {
            if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, false)
            {
                return ItemStack::empty();
            }
        } else if self
            .state
            .borrow()
            .recipes
            .stonecutter_recipes()
            .accepts_input(&stack)
        {
            if !self.move_item_stack_to(player, &mut stack, INPUT_SLOT, INPUT_SLOT + 1, false) {
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
        let moved = finish_quick_move(self, player, slot_index, stack.clone(), &clicked);
        if moved.is_empty() {
            return moved;
        }
        if slot_index == RESULT_SLOT {
            player.drop(stack, false, Prediction::Predicted);
        }
        self.broadcast_changes(player);
        moved
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        self.state
            .borrow()
            .result
            .borrow_mut()
            .remove_item_no_update(0);
        if self.access.pos().is_some() {
            self.clear_container(player, INPUT_LOCAL);
        }
    }
}
