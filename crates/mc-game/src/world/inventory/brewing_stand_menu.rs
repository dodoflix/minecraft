//! `BrewingStandMenu`: 3 potion slots, ingredient, fuel (all backed by the brewing stand block entity) + 4 data slots
//! (brew time, fuel, total brew time, total fuel) + the player inventory.

use super::{AbstractContainerMenu, ContainerKey, MenuBase, MenuPlayer, Slot, SlotBehavior, SlotEnv};
use crate::world::item::crafting::{PotionIngredient, RecipePropertySet};
use crate::world::item::{ItemStack, ItemStackExt};
use mc_core::pos::BlockPos;
use mc_data::{DataComponentType, MenuType};
use mc_protocol::components::POTION_CONTENTS;
use std::any::Any;

const INGREDIENT_SLOT: usize = 3;
const FUEL_SLOT: usize = 4;
const INV_SLOT_START: usize = 5;
const INV_SLOT_END: usize = 32;
const USE_ROW_SLOT_START: usize = 32;
const USE_ROW_SLOT_END: usize = 41;

/// `FuelSlot.mayPlaceItem`.
fn fuel_may_place(stack: &ItemStack) -> bool {
    stack.has(DataComponentType::BREWING_FUEL)
}

/// `recipeAccess.propertySet(BREWING_REAGENTS).test(stack)` / `PotionIngredient.isPotionInput(stack, recipeAccess)`
/// against the recipes of the player's level.
fn recipe_test(env: &mut SlotEnv, test: impl FnOnce(&crate::world::item::crafting::RecipeManager) -> bool) -> bool {
    env.player.level().is_some_and(|level| test(&level.recipe_access()))
}

struct FuelSlot;

impl SlotBehavior for FuelSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        fuel_may_place(stack)
    }
}

struct IngredientsSlot;

impl SlotBehavior for IngredientsSlot {
    fn may_place(&self, _slot: &Slot, env: &mut SlotEnv, stack: &ItemStack) -> bool {
        recipe_test(env, |recipes| recipes.property_set(RecipePropertySet::BREWING_REAGENTS).test(stack))
    }
}

struct PotionSlot;

impl SlotBehavior for PotionSlot {
    fn may_place(&self, _slot: &Slot, env: &mut SlotEnv, stack: &ItemStack) -> bool {
        recipe_test(env, |recipes| PotionIngredient::is_potion_input(stack, recipes))
    }

    fn get_max_stack_size(&self, _slot: &Slot, _env: &mut SlotEnv) -> i32 {
        1
    }

    fn on_take(&self, slot: &Slot, env: &mut SlotEnv, carried: &ItemStack) {
        if let Some(contents) = carried.get(&POTION_CONTENTS) {
            env.player.trigger_brewed_potion(contents);
        }
        slot.set_changed(env);
    }
}

pub struct BrewingStandMenu {
    base: MenuBase,
}

impl BrewingStandMenu {
    /// `new BrewingStandMenu(containerId, inventory, brewingStand, brewingStandData)` for the brewing stand at `pos`.
    pub fn new(container_id: i32, pos: BlockPos) -> Self {
        let mut base = MenuBase::new(Some(MenuType::BREWING_STAND), container_id);
        let stand = ContainerKey::BlockEntity(pos);
        base.add_slot(Slot::with_behavior(stand.clone(), 0, 56, 51, Box::new(PotionSlot)));
        base.add_slot(Slot::with_behavior(stand.clone(), 1, 79, 58, Box::new(PotionSlot)));
        base.add_slot(Slot::with_behavior(stand.clone(), 2, 102, 51, Box::new(PotionSlot)));
        base.add_slot(Slot::with_behavior(stand.clone(), INGREDIENT_SLOT, 79, 17, Box::new(IngredientsSlot)));
        base.add_slot(Slot::with_behavior(stand, FUEL_SLOT, 17, 17, Box::new(FuelSlot)));
        base.add_block_entity_data_slots(pos, 4);
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self { base }
    }

    /// Writes the (moved-from) working copy back into the clicked slot without any `setChanged` side effect: vanilla
    /// mutates the slot's own `ItemStack` object, which is what makes its early `return ItemStack.EMPTY` paths still
    /// move items.
    fn write_back(&mut self, player: &mut dyn MenuPlayer, slot_index: usize, stack: &ItemStack) {
        let rest = stack.clone();
        self.base.with_slot(player, slot_index, |s, e| s.with_item_mut(e, |t| *t = rest));
    }
}

impl AbstractContainerMenu for BrewingStandMenu {
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
        match self.base.slots[0].container {
            ContainerKey::BlockEntity(pos) => crate::world::container::still_valid_block_entity(pos, player),
            _ => false,
        }
    }

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        let mut clicked = ItemStack::empty();
        if !self.base.with_slot(player, slot_index, |s, e| s.has_item(e)) {
            return clicked;
        }
        let mut stack = self.base.with_slot(player, slot_index, |s, e| s.get_item(e));
        clicked = stack.copy();
        if !(slot_index <= 2) && slot_index != INGREDIENT_SLOT && slot_index != FUEL_SLOT {
            if fuel_may_place(&clicked) {
                let into_fuel = self.move_item_stack_to(player, &mut stack, FUEL_SLOT, FUEL_SLOT + 1, false);
                let early = into_fuel || (self.base.with_slot(player, INGREDIENT_SLOT, |s, e| s.may_place(e, &stack)) && !self.move_item_stack_to(player, &mut stack, INGREDIENT_SLOT, INGREDIENT_SLOT + 1, false));
                if early {
                    self.write_back(player, slot_index, &stack);
                    return ItemStack::empty();
                }
            } else if self.base.with_slot(player, INGREDIENT_SLOT, |s, e| s.may_place(e, &stack)) {
                if !self.move_item_stack_to(player, &mut stack, INGREDIENT_SLOT, INGREDIENT_SLOT + 1, false) {
                    return ItemStack::empty();
                }
            } else if self.base.with_slot(player, 0, |s, e| recipe_test(e, |recipes| PotionIngredient::is_potion_input(&clicked, recipes))) {
                if !self.move_item_stack_to(player, &mut stack, 0, 3, false) {
                    return ItemStack::empty();
                }
            } else if (INV_SLOT_START..INV_SLOT_END).contains(&slot_index) {
                if !self.move_item_stack_to(player, &mut stack, USE_ROW_SLOT_START, USE_ROW_SLOT_END, false) {
                    return ItemStack::empty();
                }
            } else if (USE_ROW_SLOT_START..USE_ROW_SLOT_END).contains(&slot_index) {
                if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, INV_SLOT_END, false) {
                    return ItemStack::empty();
                }
            } else if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, false) {
                return ItemStack::empty();
            }
        } else {
            if !self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true) {
                return ItemStack::empty();
            }
            self.base.with_slot(player, slot_index, |s, e| s.on_quick_craft(e, &stack, &clicked));
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
        self.base.with_slot(player, slot_index, |s, e| s.on_take(e, &clicked));
        self.flush_container_events(player);
        clicked
    }
}
