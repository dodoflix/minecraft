//! `CraftingMenu` (the crafting table's 3x3 menu) over `AbstractCraftingMenu`.

use super::{
    AbstractContainerMenu, ContainerKey, ContainerLevelAccess, MenuBase, MenuPlayer, Prediction,
    ResultContainer, Slot, TransientCraftingContainer, result_slot, slot_changed_crafting_grid,
    still_valid,
};
use crate::world::item::ItemStack;
use crate::world::item::behaviour;
use crate::world::item::crafting::{
    CraftingGrid, PostPlaceAction, RecipeBookMenu, RecipeHolder, StackedItemContents,
    handle_crafting_placement,
};
use mc_data::{Block, MenuType};
use mc_protocol::packets::play::RecipeBookType;
use std::any::Any;

pub const RESULT_SLOT: usize = 0;
const CRAFT_SLOT_START: usize = 1;
const CRAFT_SLOT_END: usize = 10;
const INV_SLOT_START: usize = 10;
const INV_SLOT_END: usize = 37;
const USE_ROW_SLOT_START: usize = 37;
const USE_ROW_SLOT_END: usize = 46;

/// Menu-local container indices.
const CRAFT_LOCAL: usize = 0;
const RESULT_LOCAL: usize = 1;

pub struct CraftingMenu {
    base: MenuBase,
    access: ContainerLevelAccess,
    placing_recipe: bool,
}

impl CraftingMenu {
    /// `CraftingMenu(containerId, inventory, access)`.
    pub fn new(container_id: i32, access: ContainerLevelAccess) -> Self {
        let mut base = MenuBase::new(Some(MenuType::CRAFTING), container_id);
        let craft = base.add_local(Box::new(TransientCraftingContainer::new(3, 3)));
        let result = base.add_local(Box::new(ResultContainer::new()));
        debug_assert_eq!((craft, result), (CRAFT_LOCAL, RESULT_LOCAL));
        base.add_slot(result_slot(RESULT_LOCAL, CRAFT_LOCAL, 124, 35));
        // `addCraftingGridSlots(30, 17)`
        for y in 0..3 {
            for x in 0..3 {
                base.add_slot(Slot::new(
                    ContainerKey::Local(CRAFT_LOCAL),
                    (x + y * 3) as usize,
                    30 + x * 18,
                    17 + y * 18,
                ));
            }
        }
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self {
            base,
            access,
            placing_recipe: false,
        }
    }

    pub fn get_craft_slots(&self) -> &TransientCraftingContainer {
        self.base.locals[CRAFT_LOCAL]
            .as_any()
            .downcast_ref()
            .expect("craft slots")
    }

    pub fn get_result_slot(&self) -> &Slot {
        &self.base.slots[RESULT_SLOT]
    }

    /// `getInputGridSlots()` + the grid size of `AbstractCraftingMenu`.
    fn crafting_grid() -> CraftingGrid {
        CraftingGrid {
            craft_local: CRAFT_LOCAL,
            result_local: RESULT_LOCAL,
            input_slots: CRAFT_SLOT_START..CRAFT_SLOT_END,
            width: 3,
            height: 3,
        }
    }
}

impl RecipeBookMenu for CraftingMenu {
    fn handle_placement(
        &mut self,
        player: &mut dyn MenuPlayer,
        use_max_items: bool,
        allow_dropping_items_to_clear: bool,
        recipe: &RecipeHolder,
    ) -> PostPlaceAction {
        // `beginPlacingRecipe()`
        self.placing_recipe = true;
        let action = handle_crafting_placement(
            self,
            &Self::crafting_grid(),
            player,
            use_max_items,
            allow_dropping_items_to_clear,
            recipe,
        );
        // `finishPlacingRecipe(level, recipe)`: the one recompute of the placement, hinted with the placed recipe.
        self.placing_recipe = false;
        // the hinted lookup calls `matches` of a recipe of another class with a crafting input: vanilla's cast throws there
        // (suppressed by the packet processor), so there is no recompute and no ghost recipe
        if !recipe.value.is_crafting_recipe() {
            return PostPlaceAction::Nothing;
        }
        let items = self.get_craft_slots().get_items().to_vec();
        slot_changed_crafting_grid(
            &mut self.base,
            player,
            CRAFT_LOCAL,
            RESULT_LOCAL,
            &items,
            Some(recipe),
        );
        action
    }

    fn fill_craft_slots_stacked_contents(
        &mut self,
        _player: &mut dyn MenuPlayer,
        contents: &mut StackedItemContents,
    ) {
        self.get_craft_slots().fill_stacked_contents(contents);
    }

    fn get_recipe_book_type(&self) -> RecipeBookType {
        RecipeBookType::Crafting
    }
}

impl AbstractContainerMenu for CraftingMenu {
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

    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, container: &ContainerKey) {
        // The grid's own mutations replay the grid as it was right after them (also while placing, which skips the
        // recompute: the queue must stay aligned); any other caller sees the grid as it is now.
        let items = if matches!(container, ContainerKey::Local(CRAFT_LOCAL)) {
            self.base.locals[CRAFT_LOCAL]
                .as_any_mut()
                .downcast_mut::<TransientCraftingContainer>()
                .expect("craft slots")
                .take_snapshot()
        } else {
            self.get_craft_slots().get_items().to_vec()
        };
        // `access.execute(..)` with a `ServerLevel`: nothing for the NULL access
        if !self.placing_recipe
            && self.access != ContainerLevelAccess::Null
            && player.is_server_player()
        {
            slot_changed_crafting_grid(
                &mut self.base,
                player,
                CRAFT_LOCAL,
                RESULT_LOCAL,
                &items,
                None,
            );
        }
    }

    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
        if self.access != ContainerLevelAccess::Null {
            self.clear_container(player, CRAFT_LOCAL);
        }
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        still_valid(&self.access, player, Block::CRAFTING_TABLE)
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
            // vanilla edits the slot's own stack: a crafted map keeps its post-processing even when nothing moves
            let processed = stack.clone();
            self.base.with_slot(player, RESULT_SLOT, |s, e| {
                s.with_item_mut(e, |t| *t = processed)
            });
            if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true)
            {
                return ItemStack::empty();
            }
            self.base.with_slot(player, RESULT_SLOT, |s, e| {
                s.on_quick_craft(e, &stack, &clicked)
            });
        } else if (INV_SLOT_START..USE_ROW_SLOT_END).contains(&slot_index) {
            if !self.move_item_stack_to(player, &mut stack, CRAFT_SLOT_START, CRAFT_SLOT_END, false)
            {
                let moved = if slot_index < INV_SLOT_END {
                    self.move_item_stack_to(
                        player,
                        &mut stack,
                        USE_ROW_SLOT_START,
                        USE_ROW_SLOT_END,
                        false,
                    )
                } else {
                    self.move_item_stack_to(player, &mut stack, INV_SLOT_START, INV_SLOT_END, false)
                };
                if !moved {
                    return ItemStack::empty();
                }
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
        if stack.is_empty() {
            self.base.with_slot(player, slot_index, |s, e| {
                s.set_by_player_with(e, ItemStack::empty(), &clicked)
            });
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
        // `onTake` edits the taken stack itself (a crafted map is post-processed), which is then the one dropped.
        self.base
            .with_slot(player, slot_index, |s, e| s.on_take(e, &mut stack));
        self.flush_container_events(player);
        if slot_index == RESULT_SLOT {
            player.drop(stack, false, Prediction::Predicted);
        }
        clicked
    }

    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, target: &Slot) -> bool {
        !matches!(target.container, ContainerKey::Local(i) if i == RESULT_LOCAL)
    }
}
