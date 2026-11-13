//! `CrafterMenu` with `CrafterSlot` and `NonInteractiveResultSlot`.
//!
//! The nine grid slots are the crafter block entity's own container; the result slot is a preview of what the crafter
//! would dispense (`CrafterBlock.getPotentialResults`) and cannot be touched. The menu listens to itself: every slot change
//! it sees recomputes the preview.

use super::crafter_block_entity::{CONTAINER_SIZE, CrafterBlockEntity, NUM_DATA};
use crate::world::level::block::impls::crafter_block::get_potential_results;
use mc_core::pos::BlockPos;
use mc_data::MenuType;
use mc_game_core::world::inventory::{
    AbstractContainerMenu, ContainerKey, ContainerRef, MenuBase, MenuPlayer, ResultContainer, Slot,
    SlotBehavior, SlotEnv, finish_quick_move, with_player_container,
};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::BlockGetter;
use std::any::Any;

const INV_SLOT_START: usize = CONTAINER_SIZE;
const USE_ROW_SLOT_END: usize = 45;

/// `CrafterSlot`: nothing goes into a disabled slot, and every `setChanged()` is a `slotsChanged` of the menu.
struct CrafterSlot;

impl SlotBehavior for CrafterSlot {
    fn may_place(&self, slot: &Slot, env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        // `!menu.isSlotDisabled(index)`: `containerData.get(index) == 1`
        !env.with_container(&slot.container, |container| {
            container
                .as_any()
                .downcast_ref::<CrafterBlockEntity>()
                .is_some_and(|crafter| crafter.is_slot_disabled(slot.index as i32))
        })
    }

    fn set_changed(&self, slot: &Slot, env: &mut SlotEnv) {
        slot.base_set_changed(env);
        *env.slots_changed_requested = true;
    }
}

/// `NonInteractiveResultSlot`: shows an item, never takes or accepts one.
struct NonInteractiveResultSlot;

impl SlotBehavior for NonInteractiveResultSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, _stack: &ItemStack) -> bool {
        false
    }
    fn may_pickup(&self, _slot: &Slot, _env: &mut SlotEnv) -> bool {
        false
    }
    fn allow_modification(&self, _slot: &Slot, _env: &mut SlotEnv) -> bool {
        false
    }
    fn remove(&self, _slot: &Slot, _env: &mut SlotEnv, _amount: i32) -> ItemStack {
        ItemStack::empty()
    }
    fn on_take(&self, _slot: &Slot, _env: &mut SlotEnv, _carried: &mut ItemStack) {}
    fn is_fake(&self) -> bool {
        true
    }
}

pub struct CrafterMenu {
    base: MenuBase,
    container: ContainerRef,
    pos: BlockPos,
    /// The `MenuBase::locals` index of the `ResultContainer`.
    result: usize,
}

impl CrafterMenu {
    /// `new CrafterMenu(containerId, inventory, crafter, crafter.containerData)` for the crafter at `pos`.
    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, pos: BlockPos) -> Self {
        let mut base = MenuBase::new(Some(MenuType::CRAFTER_3X3), container_id);
        let container = ContainerRef::block_entity(pos, CONTAINER_SIZE);
        container.start_open(player);
        for y in 0..3 {
            for x in 0..3 {
                base.add_slot(Slot::with_behavior(
                    ContainerKey::BlockEntity(pos),
                    x + y * 3,
                    26 + x as i32 * 18,
                    17 + y as i32 * 18,
                    Box::new(CrafterSlot),
                ));
            }
        }
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        let result = base.add_local(Box::new(ResultContainer::new()));
        base.add_slot(Slot::with_behavior(
            ContainerKey::Local(result),
            0,
            134,
            35,
            Box::new(NonInteractiveResultSlot),
        ));
        base.add_block_entity_data_slots(pos, NUM_DATA);
        let mut menu = Self {
            base,
            container,
            pos,
            result,
        };
        menu.refresh_recipe_result(player);
        menu.add_self_slot_listener(player);
        menu
    }

    /// `refreshRecipeResult()`.
    fn refresh_recipe_result(&mut self, player: &mut dyn MenuPlayer) {
        if !player.is_server_player() {
            return;
        }
        let result = player
            .level_ref()
            .and_then(|level| {
                let crafter = level
                    .get_block_entity(self.pos)?
                    .as_any()
                    .downcast_ref::<CrafterBlockEntity>()?;
                let input = crafter.as_craft_input();
                let recipe = get_potential_results(level, &input)?;
                Some(recipe.value.assemble_crafting(&input))
            })
            .unwrap_or_default();
        self.base.locals[self.result].set_item(0, result);
    }
}

impl AbstractContainerMenu for CrafterMenu {
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
        self.container.still_valid(player)
    }

    fn slot_changed_listener(
        &mut self,
        player: &mut dyn MenuPlayer,
        _slot_index: usize,
        _stack: &ItemStack,
    ) {
        self.refresh_recipe_result(player);
    }

    /// `crafterBlockEntity.setSlotState(slotId, enabled)` on the crafter this menu shows.
    fn set_container_slot_state(
        &mut self,
        player: &mut dyn MenuPlayer,
        slot_id: i32,
        enabled: bool,
    ) {
        with_player_container(player, &ContainerKey::BlockEntity(self.pos), |container| {
            if let Some(crafter) = container.as_any_mut().downcast_mut::<CrafterBlockEntity>() {
                crafter.set_slot_state(slot_id, enabled);
            }
        });
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
        let moved = if slot_index < CONTAINER_SIZE {
            self.move_item_stack_to(player, &mut stack, INV_SLOT_START, USE_ROW_SLOT_END, true)
        } else {
            self.move_item_stack_to(player, &mut stack, 0, CONTAINER_SIZE, false)
        };
        if !moved {
            return ItemStack::empty();
        }
        finish_quick_move(self, player, slot_index, stack, &clicked)
    }
}
