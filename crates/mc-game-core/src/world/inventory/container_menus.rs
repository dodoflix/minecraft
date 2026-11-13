//! `ChestMenu`, `ShulkerBoxMenu` (+`ShulkerBoxSlot`), `HopperMenu`, `DispenserMenu`: menus that show one container
//! above the player's inventory. The container is a [`ContainerRef`] (block entity / entity / ender chest / compound).

use super::{
    AbstractContainerMenu, ContainerKey, ContainerRef, MenuBase, MenuPlayer, Slot, SlotBehavior,
    SlotEnv,
};
use crate::world::item::{ItemStack, behaviour};
use mc_data::MenuType;
use std::any::Any;

/// The shared `quickMoveStack` of these menus: items move between the container slots `[0, container_size)` and the
/// rest of the menu (backwards into the player's inventory, forwards into the container). `dispenser_checks` adds
/// `DispenserMenu`'s "nothing moved" bail-out and `onTake`.
fn quick_move_container(
    menu: &mut dyn AbstractContainerMenu,
    player: &mut dyn MenuPlayer,
    slot_index: usize,
    container_size: usize,
    total_slots: usize,
    dispenser_checks: bool,
) -> ItemStack {
    let mut clicked = ItemStack::empty();
    let has = menu
        .base_mut()
        .with_slot(player, slot_index, |s, e| s.has_item(e));
    if !has {
        return clicked;
    }
    // Work on a copy of the slot's stack; the remainder is written back below (see `InventoryMenu::quick_move_stack`).
    let mut stack = menu
        .base_mut()
        .with_slot(player, slot_index, |s, e| s.get_item(e));
    clicked = stack.copy();
    let moved = if slot_index < container_size {
        menu.move_item_stack_to(player, &mut stack, container_size, total_slots, true)
    } else {
        menu.move_item_stack_to(player, &mut stack, 0, container_size, false)
    };
    if !moved {
        return ItemStack::empty();
    }
    if stack.is_empty() {
        menu.base_mut().with_slot(player, slot_index, |s, e| {
            s.set_by_player_with(e, ItemStack::empty(), &clicked)
        });
    } else {
        let rest = stack.clone();
        menu.base_mut().with_slot(player, slot_index, |s, e| {
            s.with_item_mut(e, |t| *t = rest);
            s.set_changed(e);
        });
    }
    if dispenser_checks {
        if stack.count() == clicked.count() {
            return ItemStack::empty();
        }
        menu.base_mut()
            .with_slot(player, slot_index, |s, e| s.on_take(e, &mut stack));
    }
    menu.flush_container_events(player);
    clicked
}

macro_rules! container_menu_boilerplate {
    () => {
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
        fn removed(&mut self, player: &mut dyn MenuPlayer) {
            self.base_removed(player);
            self.container.stop_open(player);
        }
    };
}

fn add_container_slots(
    base: &mut MenuBase,
    container: &ContainerRef,
    size: usize,
    columns: usize,
    left: i32,
    top: i32,
    behavior: impl Fn() -> Option<Box<dyn SlotBehavior>>,
) {
    for i in 0..size {
        let (x, y) = ((i % columns) as i32, (i / columns) as i32);
        let (key, index) = container.locate(i);
        let (sx, sy) = (left + x * 18, top + y * 18);
        match behavior() {
            Some(b) => base.add_slot(Slot::with_behavior(key, index, sx, sy, b)),
            None => base.add_slot(Slot::new(key, index, sx, sy)),
        };
    }
}

/// `ChestMenu`.
pub struct ChestMenu {
    base: MenuBase,
    container: ContainerRef,
    container_rows: usize,
}

impl ChestMenu {
    /// `ChestMenu(menuType, containerId, inventory, container, rows)`.
    pub fn new(
        menu_type: MenuType,
        container_id: i32,
        player: &mut dyn MenuPlayer,
        container: ContainerRef,
        rows: usize,
    ) -> Self {
        let mut base = MenuBase::new(Some(menu_type), container_id);
        container.check_size(rows * 9);
        container.start_open(player);
        add_container_slots(&mut base, &container, rows * 9, 9, 8, 18, || None);
        let inventory_top = 18 + rows as i32 * 18 + 13;
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, inventory_top);
        Self {
            base,
            container,
            container_rows: rows,
        }
    }

    pub fn one_row(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        container: ContainerRef,
    ) -> Self {
        Self::new(MenuType::GENERIC_9X1, container_id, player, container, 1)
    }
    pub fn two_rows(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        container: ContainerRef,
    ) -> Self {
        Self::new(MenuType::GENERIC_9X2, container_id, player, container, 2)
    }
    /// `ChestMenu.threeRows(containerId, inventory, container)`.
    pub fn three_rows(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        container: ContainerRef,
    ) -> Self {
        Self::new(MenuType::GENERIC_9X3, container_id, player, container, 3)
    }
    pub fn four_rows(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        container: ContainerRef,
    ) -> Self {
        Self::new(MenuType::GENERIC_9X4, container_id, player, container, 4)
    }
    pub fn five_rows(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        container: ContainerRef,
    ) -> Self {
        Self::new(MenuType::GENERIC_9X5, container_id, player, container, 5)
    }
    /// `ChestMenu.sixRows(containerId, inventory, container)` (double chest).
    pub fn six_rows(
        container_id: i32,
        player: &mut dyn MenuPlayer,
        container: ContainerRef,
    ) -> Self {
        Self::new(MenuType::GENERIC_9X6, container_id, player, container, 6)
    }

    pub fn get_container(&self) -> &ContainerRef {
        &self.container
    }

    pub fn get_row_count(&self) -> usize {
        self.container_rows
    }
}

impl AbstractContainerMenu for ChestMenu {
    container_menu_boilerplate!();

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        let (total, size) = (self.base.slots.len(), self.container_rows * 9);
        quick_move_container(self, player, slot_index, size, total, false)
    }
}

/// `ShulkerBoxSlot`: `mayPlace` = `item.canFitInsideContainerItems()`.
struct ShulkerBoxSlot;

impl SlotBehavior for ShulkerBoxSlot {
    fn may_place(&self, _slot: &Slot, _env: &mut SlotEnv, stack: &ItemStack) -> bool {
        behaviour(stack.item()).can_fit_inside_container_items()
    }
}

/// `ShulkerBoxMenu`.
pub struct ShulkerBoxMenu {
    base: MenuBase,
    container: ContainerRef,
}

impl ShulkerBoxMenu {
    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, container: ContainerRef) -> Self {
        let mut base = MenuBase::new(Some(MenuType::SHULKER_BOX), container_id);
        container.check_size(27);
        container.start_open(player);
        add_container_slots(&mut base, &container, 27, 9, 8, 18, || {
            Some(Box::new(ShulkerBoxSlot))
        });
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self { base, container }
    }
}

impl AbstractContainerMenu for ShulkerBoxMenu {
    container_menu_boilerplate!();

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        let (total, size) = (self.base.slots.len(), self.container.size());
        quick_move_container(self, player, slot_index, size, total, false)
    }
}

/// `HopperMenu`.
pub struct HopperMenu {
    base: MenuBase,
    container: ContainerRef,
}

impl HopperMenu {
    pub const CONTAINER_SIZE: usize = 5;

    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, hopper: ContainerRef) -> Self {
        let mut base = MenuBase::new(Some(MenuType::HOPPER), container_id);
        hopper.check_size(5);
        hopper.start_open(player);
        add_container_slots(&mut base, &hopper, 5, 5, 44, 20, || None);
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 51);
        Self {
            base,
            container: hopper,
        }
    }
}

impl AbstractContainerMenu for HopperMenu {
    container_menu_boilerplate!();

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        let (total, size) = (self.base.slots.len(), self.container.size());
        quick_move_container(self, player, slot_index, size, total, false)
    }
}

/// `DispenserMenu` (also used for droppers).
pub struct DispenserMenu {
    base: MenuBase,
    container: ContainerRef,
}

impl DispenserMenu {
    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, dispenser: ContainerRef) -> Self {
        let mut base = MenuBase::new(Some(MenuType::GENERIC_3X3), container_id);
        dispenser.check_size(9);
        dispenser.start_open(player);
        add_container_slots(&mut base, &dispenser, 9, 3, 62, 17, || None);
        base.add_standard_inventory_slots(&ContainerKey::PlayerInventory, 8, 84);
        Self {
            base,
            container: dispenser,
        }
    }
}

impl AbstractContainerMenu for DispenserMenu {
    container_menu_boilerplate!();

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack {
        quick_move_container(self, player, slot_index, 9, 45, true)
    }
}
