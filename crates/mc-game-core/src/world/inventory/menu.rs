//! `AbstractContainerMenu`: slots, click algorithms, remote synchronisation.
//!
//! Vanilla's abstract class becomes [`MenuBase`] (all state) + [`AbstractContainerMenu`] (trait: the abstract and
//! overridable methods, plus the non-overridden algorithms as provided methods). Every method that reads slot
//! contents takes the player because slots reach their container through it (see the module docs).
//!
//! Deviations (all unobservable on the wire/disk): `quickcraftSlots` is insertion-ordered (Java iterates an identity
//! hash set; the drag result does not depend on the order); containers that call back into the menu from
//! `setItem`/`removeItem` (crafting grids) queue the callback and the menu replays it after each slot operation
//! (`flush_container_events`) — one `slotsChanged` per mutation, in order.

use super::{
    ClickAction, ContainerInput, ContainerKey, ContainerListener, ContainerSynchronizer, DataSlot,
    MenuPlayer, Prediction, RemoteSlot, Slot, SlotEnv, place_item_back_in_inventory,
};
use crate::world::container::Container;
use crate::world::item::{HashedStack, ItemStack};
use mc_data::MenuType;
use std::any::Any;

pub const SLOT_CLICKED_OUTSIDE: i32 = -999;
pub const CONTAINER_CLICK_PRIMARY: i32 = 0;
pub const CONTAINER_CLICK_SECONDARY: i32 = 1;
pub const QUICKCRAFT_TYPE_CHARITABLE: i32 = 0;
pub const QUICKCRAFT_TYPE_GREEDY: i32 = 1;
pub const QUICKCRAFT_TYPE_CLONE: i32 = 2;
pub const QUICKCRAFT_HEADER_START: i32 = 0;
pub const QUICKCRAFT_HEADER_CONTINUE: i32 = 1;
pub const QUICKCRAFT_HEADER_END: i32 = 2;
pub const CARRIED_SLOT_SIZE: i32 = i32::MAX;
pub const SLOTS_PER_ROW: i32 = 9;
pub const SLOT_SIZE: i32 = 18;

/// What vanilla reports as an exception out of `clicked` (slot index outside `slots`); the connection reacts like it
/// does to any exception while handling a packet.
#[derive(Debug, PartialEq, Eq)]
pub struct MenuClickError {
    pub slot_index: i32,
    pub slot_count: usize,
}

pub fn get_quickcraft_type(mask: i32) -> i32 {
    mask >> 2 & 3
}
pub fn get_quickcraft_header(mask: i32) -> i32 {
    mask & 3
}
pub fn get_quickcraft_mask(header: i32, ty: i32) -> i32 {
    header & 3 | (ty & 3) << 2
}
pub fn is_valid_quickcraft_type(ty: i32, player: &dyn MenuPlayer) -> bool {
    ty == 0 || ty == 1 || (ty == 2 && player.has_infinite_materials())
}

/// `getQuickCraftPlaceCount`.
pub fn get_quick_craft_place_count(slot_count: usize, ty: i32, stack: &ItemStack) -> i32 {
    match ty {
        0 => (stack.count() as f32 / slot_count as f32).floor() as i32,
        1 => 1,
        2 => stack.get_max_stack_size(),
        _ => stack.count(),
    }
}

/// `canItemQuickReplace(slot, stack, ignoreSize)`.
pub fn can_item_quick_replace(
    slot: Option<&Slot>,
    env: &mut SlotEnv,
    stack: &ItemStack,
    ignore_size: bool,
) -> bool {
    let slot_item = slot.map(|s| s.get_item(env));
    let slot_is_empty = slot_item.as_ref().is_none_or(ItemStack::is_empty);
    match slot_item {
        Some(item) if !slot_is_empty && ItemStack::is_same_item_same_components(stack, &item) => {
            item.count() + if ignore_size { 0 } else { stack.count() } <= stack.get_max_stack_size()
        }
        _ => slot_is_empty,
    }
}

pub struct MenuBase {
    last_slots: Vec<ItemStack>,
    pub slots: Vec<Slot>,
    /// Containers owned by the menu (`ContainerKey::Local(i)`).
    pub locals: Vec<Box<dyn Container>>,
    data_slots: Vec<DataSlot>,
    carried: ItemStack,
    remote_slots: Vec<RemoteSlot>,
    remote_data_slots: Vec<i32>,
    remote_carried: RemoteSlot,
    state_id: i32,
    menu_type: Option<MenuType>,
    pub container_id: i32,
    quickcraft_type: i32,
    quickcraft_status: i32,
    quickcraft_slots: Vec<usize>,
    slots_changed_requested: bool,
    container_listeners: Vec<Box<dyn ContainerListener>>,
    synchronizer: Option<Box<dyn ContainerSynchronizer>>,
    suppress_remote_updates: bool,
    /// Slot changes found by `triggerSlotListeners` not yet handed to the menu's own listener.
    pub(crate) slot_events: Vec<(usize, ItemStack)>,
}

impl MenuBase {
    pub fn new(menu_type: Option<MenuType>, container_id: i32) -> Self {
        Self {
            last_slots: Vec::new(),
            slots: Vec::new(),
            locals: Vec::new(),
            data_slots: Vec::new(),
            carried: ItemStack::empty(),
            remote_slots: Vec::new(),
            remote_data_slots: Vec::new(),
            remote_carried: RemoteSlot::Placeholder,
            state_id: 0,
            menu_type,
            container_id,
            quickcraft_type: -1,
            quickcraft_status: 0,
            quickcraft_slots: Vec::new(),
            slots_changed_requested: false,
            container_listeners: Vec::new(),
            synchronizer: None,
            suppress_remote_updates: false,
            slot_events: Vec::new(),
        }
    }

    /// `getType()`: panics like vanilla's `UnsupportedOperationException` for typeless menus (only the inventory menu).
    pub fn get_type(&self) -> MenuType {
        self.menu_type
            .expect("Unable to construct this menu by type")
    }

    pub fn menu_type(&self) -> Option<MenuType> {
        self.menu_type
    }

    /// Registers a container owned by the menu and returns its `ContainerKey::Local` index.
    pub fn add_local(&mut self, container: Box<dyn Container>) -> usize {
        self.locals.push(container);
        self.locals.len() - 1
    }

    pub fn add_slot(&mut self, mut slot: Slot) -> usize {
        slot.index = self.slots.len();
        self.slots.push(slot);
        self.last_slots.push(ItemStack::empty());
        let remote = self
            .synchronizer
            .as_ref()
            .map_or(RemoteSlot::Placeholder, |s| s.create_slot());
        self.remote_slots.push(remote);
        self.slots.len() - 1
    }

    pub fn add_inventory_hotbar_slots(&mut self, inventory: &ContainerKey, left: i32, top: i32) {
        for x in 0..9 {
            self.add_slot(Slot::new(inventory.clone(), x as usize, left + x * 18, top));
        }
    }

    pub fn add_inventory_extended_slots(&mut self, inventory: &ContainerKey, left: i32, top: i32) {
        for y in 0..3 {
            for x in 0..9 {
                self.add_slot(Slot::new(
                    inventory.clone(),
                    (x + (y + 1) * 9) as usize,
                    left + x * 18,
                    top + y * 18,
                ));
            }
        }
    }

    pub fn add_standard_inventory_slots(&mut self, container: &ContainerKey, left: i32, top: i32) {
        self.add_inventory_extended_slots(container, left, top);
        self.add_inventory_hotbar_slots(container, left, top + 58);
    }

    pub fn add_data_slot(&mut self, data_slot: DataSlot) {
        self.data_slots.push(data_slot);
        self.remote_data_slots.push(0);
    }

    /// `addDataSlots(ContainerData)`.
    pub fn add_data_slots(&mut self, container: super::SharedContainerData) {
        let count = container.borrow().get_count();
        for i in 0..count {
            self.add_data_slot(DataSlot::for_container(container.clone(), i));
        }
    }

    /// `addDataSlots(dataAccess)` for the `ContainerData` of the block entity at `pos` (`count` ints).
    pub fn add_block_entity_data_slots(&mut self, pos: mc_core::pos::BlockPos, count: usize) {
        for i in 0..count {
            self.add_data_slot(DataSlot::for_block_entity(pos, i));
        }
    }

    fn refresh_data_slots(&mut self, player: &dyn MenuPlayer) {
        for d in &mut self.data_slots {
            d.refresh(player);
        }
    }

    /// The current value of every data slot, in registration order.
    pub fn data_slot_values(&self) -> Vec<i32> {
        self.data_slots.iter().map(DataSlot::get).collect()
    }

    pub fn is_valid_slot_index(&self, slot_index: i32) -> bool {
        slot_index == -1 || slot_index == -999 || (slot_index as i64) < self.slots.len() as i64
    }

    /// Slot `index` and an env over the menu's locals.
    pub fn with_slot<R>(
        &mut self,
        player: &mut dyn MenuPlayer,
        index: usize,
        f: impl FnOnce(&Slot, &mut SlotEnv<'_>) -> R,
    ) -> R {
        let slot = &self.slots[index];
        let mut env = SlotEnv {
            player,
            locals: &mut self.locals,
            slots_changed_requested: &mut self.slots_changed_requested,
        };
        f(slot, &mut env)
    }

    /// An env over the menu's locals (all slots readable through `self.slots`).
    pub fn env<'a>(&'a mut self, player: &'a mut dyn MenuPlayer) -> (&'a [Slot], SlotEnv<'a>) {
        (
            &self.slots,
            SlotEnv {
                player,
                locals: &mut self.locals,
                slots_changed_requested: &mut self.slots_changed_requested,
            },
        )
    }

    pub fn get_carried(&self) -> &ItemStack {
        &self.carried
    }
    pub fn set_carried(&mut self, carried: ItemStack) {
        self.carried = carried;
    }
    pub fn get_carried_mut(&mut self) -> &mut ItemStack {
        &mut self.carried
    }

    pub fn get_state_id(&self) -> i32 {
        self.state_id
    }
    pub fn increment_state_id(&mut self) -> i32 {
        self.state_id = self.state_id.wrapping_add(1) & 32767;
        self.state_id
    }

    pub fn suppress_remote_updates(&mut self) {
        self.suppress_remote_updates = true;
    }
    pub fn resume_remote_updates(&mut self) {
        self.suppress_remote_updates = false;
    }

    /// `addSlotListener` without the initial `broadcastChanges` (the trait method `add_slot_listener` runs it).
    pub(crate) fn push_slot_listener(&mut self, listener: Box<dyn ContainerListener>) {
        // Vanilla de-duplicates by identity; a listener is only ever attached once per menu by the owner.
        self.container_listeners.push(listener);
    }

    pub fn set_synchronizer(
        &mut self,
        synchronizer: Box<dyn ContainerSynchronizer>,
        player: &mut dyn MenuPlayer,
    ) {
        self.remote_carried = synchronizer.create_slot();
        for r in &mut self.remote_slots {
            *r = synchronizer.create_slot();
        }
        self.synchronizer = Some(synchronizer);
        self.send_all_data_to_remote(player);
    }

    pub fn has_synchronizer(&self) -> bool {
        self.synchronizer.is_some()
    }

    pub fn send_all_data_to_remote(&mut self, player: &mut dyn MenuPlayer) {
        self.refresh_data_slots(player);
        let mut items_to_send = Vec::with_capacity(self.slots.len());
        for i in 0..self.slots.len() {
            let contents = self.with_slot(player, i, |s, e| s.get_item(e));
            items_to_send.push(contents.copy());
            self.remote_slots[i].force(&contents);
        }
        let carried = self.carried.clone();
        self.remote_carried.force(&carried);
        for i in 0..self.data_slots.len() {
            self.remote_data_slots[i] = self.data_slots[i].get();
        }
        if self.synchronizer.is_some() {
            let state_id = self.increment_state_id();
            let container_id = self.container_id;
            let data = self.remote_data_slots.clone();
            let sync = self.synchronizer.as_mut().expect("checked above");
            sync.send_initial_data(container_id, state_id, items_to_send, carried.copy(), &data);
        }
    }

    pub fn remove_slot_listeners(&mut self) {
        self.container_listeners.clear();
    }

    pub fn get_items(&mut self, player: &mut dyn MenuPlayer) -> Vec<ItemStack> {
        (0..self.slots.len())
            .map(|i| self.with_slot(player, i, |s, e| s.get_item(e)))
            .collect()
    }

    /// Step of `broadcastChanges`/`broadcastFullState`: slot `i` as seen now.
    pub(crate) fn current_slot_item(&mut self, player: &mut dyn MenuPlayer, i: usize) -> ItemStack {
        self.with_slot(player, i, |s, e| s.get_item(e))
    }

    /// The part of `broadcastChanges` after the slot loop: carried item and data slots.
    pub(crate) fn broadcast_carried_and_data(&mut self, player: &mut dyn MenuPlayer) {
        self.synchronize_carried_to_remote();
        self.refresh_data_slots(player);
        for i in 0..self.data_slots.len() {
            let current_value = self.data_slots[i].get();
            if self.data_slots[i].check_and_clear_update_flag() {
                self.update_data_slot_listeners(i, current_value);
            }
            self.synchronize_data_slot_to_remote(i, current_value);
        }
    }

    /// The part of `broadcastFullState` after the slot loop.
    pub(crate) fn broadcast_full_state_data(&mut self, player: &mut dyn MenuPlayer) {
        self.refresh_data_slots(player);
        for i in 0..self.data_slots.len() {
            let current_value = self.data_slots[i].get();
            if self.data_slots[i].check_and_clear_update_flag() {
                self.update_data_slot_listeners(i, current_value);
            }
        }
        self.send_all_data_to_remote(player);
    }

    fn update_data_slot_listeners(&mut self, id: usize, value: i32) {
        for l in &mut self.container_listeners {
            l.data_changed(id, value);
        }
    }

    /// `triggerSlotListeners`: records the change for the menu's own `slot_changed_listener` too.
    pub(crate) fn trigger_slot_listeners(&mut self, i: usize, current: &ItemStack) {
        if !ItemStack::matches(&self.last_slots[i], current) {
            let new_item = current.copy();
            self.last_slots[i] = new_item.clone();
            for l in &mut self.container_listeners {
                l.slot_changed(&self.slots[i], i, &new_item);
            }
            self.slot_events.push((i, new_item));
        }
    }

    pub(crate) fn synchronize_slot_to_remote(&mut self, i: usize, current: &ItemStack) {
        if !self.suppress_remote_updates && !self.remote_slots[i].matches(current) {
            self.remote_slots[i].force(current);
            if self.synchronizer.is_some() {
                let state_id = self.increment_state_id();
                let container_id = self.container_id;
                if let Some(s) = self.synchronizer.as_mut() {
                    s.send_slot_change(container_id, state_id, i, current.copy());
                }
            }
        }
    }

    fn synchronize_data_slot_to_remote(&mut self, i: usize, current: i32) {
        if !self.suppress_remote_updates && self.remote_data_slots[i] != current {
            self.remote_data_slots[i] = current;
            let container_id = self.container_id;
            if let Some(s) = self.synchronizer.as_mut() {
                s.send_data_change(container_id, i, current);
            }
        }
    }

    fn synchronize_carried_to_remote(&mut self) {
        if !self.suppress_remote_updates {
            let carried = self.carried.clone();
            if !self.remote_carried.matches(&carried) {
                self.remote_carried.force(&carried);
                if let Some(s) = self.synchronizer.as_mut() {
                    s.send_carried_change(carried.copy());
                }
            }
        }
    }

    /// Sends `ClientboundContainerSetSlotPacket(containerId, incrementStateId(), slot, stack)` right away
    /// (`CraftingMenu.slotChangedCraftingGrid` bypasses the broadcast cycle).
    pub fn send_slot_change_now(&mut self, slot: usize, stack: ItemStack) {
        if self.synchronizer.is_some() {
            let state_id = self.increment_state_id();
            let container_id = self.container_id;
            if let Some(s) = self.synchronizer.as_mut() {
                s.send_slot_change(container_id, state_id, slot, stack);
            }
        }
    }

    pub fn set_remote_slot(&mut self, slot: usize, stack: &ItemStack) {
        self.remote_slots[slot].force(stack);
    }

    /// `setRemoteSlotUnsafe`: out-of-range indices are ignored (vanilla logs at debug level).
    pub fn set_remote_slot_unsafe(&mut self, slot: i32, stack: HashedStack) {
        if slot >= 0 && (slot as usize) < self.remote_slots.len() {
            self.remote_slots[slot as usize].receive(stack);
        } else {
            log::debug!(
                "Incorrect slot index: {} available slots: {}",
                slot,
                self.remote_slots.len()
            );
        }
    }

    pub fn set_remote_carried(&mut self, carried: HashedStack) {
        self.remote_carried.receive(carried);
    }

    /// `setItem(slot, stateId, stack)` (client-bound data applied; used by tests / internal resync).
    pub fn set_item(
        &mut self,
        player: &mut dyn MenuPlayer,
        slot: usize,
        state_id: i32,
        stack: ItemStack,
    ) {
        self.with_slot(player, slot, |s, e| s.set(e, stack));
        self.state_id = state_id;
    }

    pub fn set_data(&mut self, id: usize, value: i32) {
        self.data_slots[id].set(value);
    }

    /// `transferState(other)`: carry remote-slot knowledge over for slots backed by the same container slot.
    pub fn transfer_state(&mut self, other: &MenuBase) {
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if matches!(slot.container, ContainerKey::Local(_)) {
                continue;
            }
            let found = other.slots.iter().position(|o| {
                o.container.same_container(&slot.container)
                    && o.get_container_slot() == slot.get_container_slot()
            });
            if let Some(other_index) = found {
                self.last_slots[slot_index] = other.last_slots[other_index].clone();
                let src = other.remote_slots[other_index].clone();
                self.remote_slots[slot_index].copy_from(&src);
            }
        }
    }

    /// `findSlot(inventory, slotIndex)`.
    pub fn find_slot(&self, container: &ContainerKey, slot_index: usize) -> Option<usize> {
        self.slots.iter().position(|s| {
            s.container.same_container(container) && slot_index == s.get_container_slot()
        })
    }

    /// Takes the pending `slotsChanged(inventory)` request raised by item hooks.
    pub fn take_slots_changed_request(&mut self) -> bool {
        std::mem::take(&mut self.slots_changed_requested)
    }

    fn reset_quick_craft(&mut self) {
        self.quickcraft_status = 0;
        self.quickcraft_slots.clear();
    }
}

/// `AbstractContainerMenu`.
pub trait AbstractContainerMenu: Any {
    fn base(&self) -> &MenuBase;
    fn base_mut(&mut self) -> &mut MenuBase;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn quick_move_stack(&mut self, player: &mut dyn MenuPlayer, slot_index: usize) -> ItemStack;
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool;

    // ---- overridable ----

    /// `setSelectedBundleItemIndex(slotIndex, selectedItemIndex)`.
    fn set_selected_bundle_item_index(
        &mut self,
        player: &mut dyn MenuPlayer,
        slot_index: i32,
        selected_item_index: i32,
    ) {
        if slot_index >= 0 && (slot_index as usize) < self.base().slots.len() {
            self.base_mut()
                .with_slot(player, slot_index as usize, |slot, env| {
                    slot.with_item_mut(env, |stack| {
                        crate::world::item::toggle_bundle_selected_item(stack, selected_item_index)
                    });
                });
        }
    }

    /// The menu acting as its own `ContainerListener` (`CrafterMenu.slotChanged`): called in slot order right after
    /// the change was detected, inside `broadcast_changes`/`broadcast_full_state`/`add_slot_listener`.
    fn slot_changed_listener(
        &mut self,
        _player: &mut dyn MenuPlayer,
        _slot_index: usize,
        _stack: &ItemStack,
    ) {
    }

    /// Hands the recorded slot changes to [`AbstractContainerMenu::slot_changed_listener`].
    fn dispatch_slot_events(&mut self, player: &mut dyn MenuPlayer) {
        while !self.base().slot_events.is_empty() {
            let events = std::mem::take(&mut self.base_mut().slot_events);
            for (i, stack) in events {
                // `ServerPlayer.containerListener.slotChanged`: `slot.container == player.getInventory()` -> INVENTORY_CHANGED
                if matches!(
                    self.base().slots[i].container,
                    ContainerKey::PlayerInventory
                ) {
                    player.with_server_player(&mut |level, p| {
                        crate::advancements::triggers::CriteriaTriggers::INVENTORY_CHANGED
                            .trigger(level, p, &stack);
                    });
                }
                self.slot_changed_listener(player, i, &stack);
            }
        }
    }

    /// `addSlotListener(listener)`: attach and `broadcastChanges()`.
    fn add_slot_listener(
        &mut self,
        listener: Box<dyn ContainerListener>,
        player: &mut dyn MenuPlayer,
    ) {
        self.base_mut().push_slot_listener(listener);
        self.broadcast_changes(player);
    }

    /// `broadcastChanges()`.
    fn broadcast_changes(&mut self, player: &mut dyn MenuPlayer) {
        for i in 0..self.base().slots.len() {
            let current = self.base_mut().current_slot_item(player, i);
            self.base_mut().trigger_slot_listeners(i, &current);
            self.dispatch_slot_events(player);
            self.base_mut().synchronize_slot_to_remote(i, &current);
        }
        self.base_mut().broadcast_carried_and_data(player);
    }

    /// `broadcastFullState()`.
    fn broadcast_full_state(&mut self, player: &mut dyn MenuPlayer) {
        for i in 0..self.base().slots.len() {
            let current = self.base_mut().current_slot_item(player, i);
            self.base_mut().trigger_slot_listeners(i, &current);
            self.dispatch_slot_events(player);
        }
        self.base_mut().broadcast_full_state_data(player);
    }

    /// `this instanceof RecipeBookMenu` (crafting/furnace menus override).
    fn as_recipe_book_menu(
        &mut self,
    ) -> Option<&mut dyn crate::world::item::crafting::RecipeBookMenu> {
        None
    }

    fn click_menu_button(&mut self, _player: &mut dyn MenuPlayer, _button_id: i32) -> bool {
        false
    }
    fn can_take_item_for_pick_all(&self, _carried: &ItemStack, _target: &Slot) -> bool {
        true
    }
    fn can_drag_to(&self, _slot: &Slot) -> bool {
        true
    }
    /// `slotsChanged(container)`: `container` is the key of the container that changed.
    fn slots_changed(&mut self, player: &mut dyn MenuPlayer, _container: &ContainerKey) {
        self.broadcast_changes(player);
    }
    fn removed(&mut self, player: &mut dyn MenuPlayer) {
        self.base_removed(player);
    }

    // ---- base implementations / algorithms ----

    /// `AbstractContainerMenu.removed`.
    fn base_removed(&mut self, player: &mut dyn MenuPlayer) {
        if player.is_server_player() {
            let carried = self.base().get_carried().clone();
            if !carried.is_empty() {
                drop_or_place_in_inventory(player, carried);
                self.base_mut().set_carried(ItemStack::empty());
            }
        }
    }

    /// `clearContainer(player, container)` for a menu-local container.
    fn clear_container(&mut self, player: &mut dyn MenuPlayer, local: usize) {
        let size = self.base().locals[local].get_container_size();
        for i in 0..size {
            let stack = self.base_mut().locals[local].remove_item_no_update(i);
            drop_or_place_in_inventory(player, stack);
        }
    }

    /// Replays the `slotsChanged` callbacks queued by menu-local containers.
    fn flush_container_events(&mut self, player: &mut dyn MenuPlayer) {
        loop {
            let mut pending = Vec::new();
            for (i, c) in self.base_mut().locals.iter_mut().enumerate() {
                for _ in 0..c.take_menu_notifications() {
                    pending.push(i);
                }
            }
            if pending.is_empty() {
                break;
            }
            for i in pending {
                self.slots_changed(player, &ContainerKey::Local(i));
            }
        }
    }

    /// `initializeContents(stateId, items, carried)`.
    fn initialize_contents(
        &mut self,
        player: &mut dyn MenuPlayer,
        state_id: i32,
        items: Vec<ItemStack>,
        carried: ItemStack,
    ) {
        for (i, item) in items.into_iter().enumerate() {
            self.base_mut().with_slot(player, i, |s, e| s.set(e, item));
        }
        let base = self.base_mut();
        base.set_carried(carried);
        base.state_id = state_id;
    }

    /// `moveItemStackTo(stack, startSlot, endSlot, backwards)`: `stack` is the (detached) stack being moved.
    fn move_item_stack_to(
        &mut self,
        player: &mut dyn MenuPlayer,
        stack: &mut ItemStack,
        start_slot: usize,
        end_slot: usize,
        backwards: bool,
    ) -> bool {
        let mut anything_changed = false;
        let mut dest = if backwards {
            end_slot as i64 - 1
        } else {
            start_slot as i64
        };
        let in_range = |d: i64| {
            if backwards {
                d >= start_slot as i64
            } else {
                d < end_slot as i64
            }
        };

        if stack.is_stackable() {
            while !stack.is_empty() && in_range(dest) {
                let (total, max, target_count, same) =
                    self.base_mut()
                        .with_slot(player, dest as usize, |slot, env| {
                            let target = slot.get_item(env);
                            let same = !target.is_empty()
                                && ItemStack::is_same_item_same_components(stack, &target);
                            let max = if same {
                                slot.get_max_stack_size_for(env, &target)
                            } else {
                                0
                            };
                            (target.count() + stack.count(), max, target.count(), same)
                        });
                if same {
                    if total <= max {
                        stack.set_count(0);
                        self.base_mut()
                            .with_slot(player, dest as usize, |slot, env| {
                                slot.with_item_mut(env, |t| t.set_count(total));
                                slot.set_changed(env);
                            });
                        anything_changed = true;
                    } else if target_count < max {
                        stack.shrink(max - target_count);
                        self.base_mut()
                            .with_slot(player, dest as usize, |slot, env| {
                                slot.with_item_mut(env, |t| t.set_count(max));
                                slot.set_changed(env);
                            });
                        anything_changed = true;
                    }
                    self.flush_container_events(player);
                }
                dest += if backwards { -1 } else { 1 };
            }
        }

        if !stack.is_empty() {
            dest = if backwards {
                end_slot as i64 - 1
            } else {
                start_slot as i64
            };
            while in_range(dest) {
                let placed = self
                    .base_mut()
                    .with_slot(player, dest as usize, |slot, env| {
                        let target = slot.get_item(env);
                        if target.is_empty() && slot.may_place(env, stack) {
                            let max = slot.get_max_stack_size_for(env, stack);
                            let part = stack.split(stack.count().min(max));
                            slot.set_by_player(env, part);
                            slot.set_changed(env);
                            true
                        } else {
                            false
                        }
                    });
                if placed {
                    self.flush_container_events(player);
                    anything_changed = true;
                    break;
                }
                dest += if backwards { -1 } else { 1 };
            }
        }
        anything_changed
    }

    /// `clicked(slotIndex, buttonNum, containerInput, player)`.
    fn clicked(
        &mut self,
        slot_index: i32,
        button_num: i32,
        input: ContainerInput,
        player: &mut dyn MenuPlayer,
    ) -> Result<(), MenuClickError> {
        self.do_click(slot_index, button_num, input, player)
    }

    fn do_click(
        &mut self,
        slot_index: i32,
        button_num: i32,
        input: ContainerInput,
        player: &mut dyn MenuPlayer,
    ) -> Result<(), MenuClickError> {
        let slot_count = self.base().slots.len();
        let slot_checked = |i: i32| -> Result<usize, MenuClickError> {
            if i >= 0 && (i as usize) < slot_count {
                Ok(i as usize)
            } else {
                Err(MenuClickError {
                    slot_index: i,
                    slot_count,
                })
            }
        };

        if input == ContainerInput::QuickCraft {
            let expected_status = self.base().quickcraft_status;
            let new_status = get_quickcraft_header(button_num);
            self.base_mut().quickcraft_status = new_status;
            if ((expected_status != 1 || new_status != 2) && expected_status != new_status)
                || self.base().carried.is_empty()
            {
                self.base_mut().reset_quick_craft();
            } else if new_status == 0 {
                let ty = get_quickcraft_type(button_num);
                self.base_mut().quickcraft_type = ty;
                if is_valid_quickcraft_type(ty, player) {
                    let base = self.base_mut();
                    base.quickcraft_status = 1;
                    base.quickcraft_slots.clear();
                } else {
                    self.base_mut().reset_quick_craft();
                }
            } else if new_status == 1 {
                let idx = slot_checked(slot_index)?;
                let carried = self.base().carried.clone();
                let ty = self.base().quickcraft_type;
                let count = self.base().quickcraft_slots.len();
                let ok = {
                    let this: &Self = &*self;
                    let may_drag = this.can_drag_to(&this.base().slots[idx]);
                    let base = self.base_mut();
                    base.with_slot(player, idx, |slot, env| {
                        can_item_quick_replace(Some(slot), env, &carried, true)
                            && slot.may_place(env, &carried)
                            && (ty == 2 || carried.count() > count as i32)
                    }) && may_drag
                };
                if ok && !self.base().quickcraft_slots.contains(&idx) {
                    self.base_mut().quickcraft_slots.push(idx);
                }
            } else if new_status == 2 {
                if !self.base().quickcraft_slots.is_empty() {
                    if self.base().quickcraft_slots.len() == 1 {
                        let slot = self.base().quickcraft_slots[0];
                        let ty = self.base().quickcraft_type;
                        self.base_mut().reset_quick_craft();
                        return self.do_click(slot as i32, ty, ContainerInput::Pickup, player);
                    }
                    let mut source = self.base().carried.copy();
                    if source.is_empty() {
                        self.base_mut().reset_quick_craft();
                        return Ok(());
                    }
                    let mut remaining = self.base().carried.count();
                    let slots = self.base().quickcraft_slots.clone();
                    let ty = self.base().quickcraft_type;
                    for idx in slots {
                        let carried = self.base().carried.clone();
                        let may_drag = self.can_drag_to(&self.base().slots[idx]);
                        let n = self.base().quickcraft_slots.len();
                        let base = self.base_mut();
                        let done = base.with_slot(player, idx, |slot, env| {
                            if can_item_quick_replace(Some(slot), env, &carried, true)
                                && slot.may_place(env, &carried)
                                && (ty == 2 || carried.count() >= n as i32)
                                && may_drag
                            {
                                let carry = if slot.has_item(env) {
                                    slot.get_item(env).count()
                                } else {
                                    0
                                };
                                let max_size = source
                                    .get_max_stack_size()
                                    .min(slot.get_max_stack_size_for(env, &source));
                                let new_count = (get_quick_craft_place_count(n, ty, &source)
                                    + carry)
                                    .min(max_size);
                                remaining -= new_count - carry;
                                slot.set_by_player(env, source.copy_with_count(new_count));
                                true
                            } else {
                                false
                            }
                        });
                        if done {
                            self.flush_container_events(player);
                        }
                    }
                    source.set_count(remaining);
                    self.base_mut().set_carried(source);
                }
                self.base_mut().reset_quick_craft();
            } else {
                self.base_mut().reset_quick_craft();
            }
        } else if self.base().quickcraft_status != 0 {
            self.base_mut().reset_quick_craft();
        } else if (input == ContainerInput::Pickup || input == ContainerInput::QuickMove)
            && (button_num == 0 || button_num == 1)
        {
            let click_action = if button_num == 0 {
                ClickAction::Primary
            } else {
                ClickAction::Secondary
            };
            if slot_index == SLOT_CLICKED_OUTSIDE {
                if !self.base().carried.is_empty() {
                    if click_action == ClickAction::Primary {
                        let carried = std::mem::take(self.base_mut().get_carried_mut());
                        player.drop(carried, true, Prediction::Predicted);
                    } else {
                        let part = self.base_mut().get_carried_mut().split(1);
                        player.drop(part, true, Prediction::Predicted);
                    }
                }
            } else if input == ContainerInput::QuickMove {
                if slot_index < 0 {
                    return Ok(());
                }
                let idx = slot_checked(slot_index)?;
                if !self
                    .base_mut()
                    .with_slot(player, idx, |s, e| s.may_pickup(e))
                {
                    return Ok(());
                }
                let mut clicked = self.quick_move_stack(player, idx);
                self.flush_container_events(player);
                while !clicked.is_empty() && {
                    let cur = self.base_mut().with_slot(player, idx, |s, e| s.get_item(e));
                    ItemStack::is_same_item(&cur, &clicked)
                } {
                    clicked = self.quick_move_stack(player, idx);
                    self.flush_container_events(player);
                }
            } else {
                if slot_index < 0 {
                    return Ok(());
                }
                let idx = slot_checked(slot_index)?;
                self.click_pickup(idx, click_action, player);
            }
        } else if input == ContainerInput::Swap
            && ((0..9).contains(&button_num) || button_num == 40)
        {
            let idx = slot_checked(slot_index)?;
            self.click_swap(idx, button_num as usize, player);
        } else if input == ContainerInput::Clone
            && player.has_infinite_materials()
            && self.base().carried.is_empty()
            && slot_index >= 0
        {
            let idx = slot_checked(slot_index)?;
            if self.base_mut().with_slot(player, idx, |s, e| s.has_item(e)) {
                let cloned = self
                    .base_mut()
                    .with_slot(player, idx, |s, e| s.safe_clone(e));
                self.base_mut().set_carried(cloned);
            }
        } else if input == ContainerInput::Throw
            && self.base().carried.is_empty()
            && slot_index >= 0
        {
            let idx = slot_checked(slot_index)?;
            let amount = if button_num == 0 {
                1
            } else {
                self.base_mut()
                    .with_slot(player, idx, |s, e| s.get_item(e))
                    .count()
            };
            if !player.can_drop_items() {
                return Ok(());
            }
            let mut item = self
                .base_mut()
                .with_slot(player, idx, |s, e| s.safe_take(e, amount, i32::MAX));
            self.flush_container_events(player);
            player.drop(item.clone(), true, Prediction::Predicted);
            player.handle_creative_mode_item_drop(&item);
            if button_num == 1 {
                while !item.is_empty() && {
                    let cur = self.base_mut().with_slot(player, idx, |s, e| s.get_item(e));
                    ItemStack::is_same_item(&cur, &item)
                } {
                    if !player.can_drop_items() {
                        return Ok(());
                    }
                    item = self
                        .base_mut()
                        .with_slot(player, idx, |s, e| s.safe_take(e, amount, i32::MAX));
                    self.flush_container_events(player);
                    player.drop(item.clone(), true, Prediction::Predicted);
                    player.handle_creative_mode_item_drop(&item);
                }
            }
        } else if input == ContainerInput::PickupAll && slot_index >= 0 {
            let idx = slot_checked(slot_index)?;
            let mut carried = self.base().carried.clone();
            let blocked = self
                .base_mut()
                .with_slot(player, idx, |s, e| !s.has_item(e) || !s.may_pickup(e));
            if !carried.is_empty() && blocked {
                let n = self.base().slots.len() as i64;
                let (start, step) = if button_num == 0 {
                    (0i64, 1i64)
                } else {
                    (n - 1, -1i64)
                };
                for pass in 0..2 {
                    let mut i = start;
                    while i >= 0 && i < n && carried.count() < carried.get_max_stack_size() {
                        let may_take = self
                            .can_take_item_for_pick_all(&carried, &self.base().slots[i as usize]);
                        let base = self.base_mut();
                        let removed = base.with_slot(player, i as usize, |target, env| {
                            if target.has_item(env)
                                && can_item_quick_replace(Some(target), env, &carried, true)
                                && target.may_pickup(env)
                                && may_take
                            {
                                let item_stack = target.get_item(env);
                                if pass != 0
                                    || item_stack.count() != item_stack.get_max_stack_size()
                                {
                                    return Some(target.safe_take(
                                        env,
                                        item_stack.count(),
                                        carried.get_max_stack_size() - carried.count(),
                                    ));
                                }
                            }
                            None
                        });
                        if let Some(removed) = removed {
                            carried.grow(removed.count());
                            self.flush_container_events(player);
                        }
                        i += step;
                    }
                }
                self.base_mut().set_carried(carried);
            }
        }
        Ok(())
    }

    /// PICKUP with a primary/secondary button on a real slot.
    fn click_pickup(&mut self, idx: usize, action: ClickAction, player: &mut dyn MenuPlayer) {
        let clicked = self.base_mut().with_slot(player, idx, |s, e| s.get_item(e));
        let mut carried = self.base().carried.clone();
        let overridden = super::item_click::try_item_click_behaviour_override(
            self.base_mut(),
            player,
            action,
            idx,
            &clicked,
            &mut carried,
        );
        if self.base_mut().take_slots_changed_request() {
            // `player.containerMenu.slotsChanged(player.getInventory())`: the carried stack is part of the state the
            // broadcast must see
            self.base_mut().set_carried(carried.clone());
            self.slots_changed(player, &ContainerKey::PlayerInventory);
        }
        if !overridden {
            if clicked.is_empty() {
                if !carried.is_empty() {
                    let amount = if action == ClickAction::Primary {
                        carried.count()
                    } else {
                        1
                    };
                    let c = std::mem::take(&mut carried);
                    carried = self
                        .base_mut()
                        .with_slot(player, idx, |s, e| s.safe_insert_amount(e, c, amount));
                }
            } else if self
                .base_mut()
                .with_slot(player, idx, |s, e| s.may_pickup(e))
            {
                if carried.is_empty() {
                    let amount = if action == ClickAction::Primary {
                        clicked.count()
                    } else {
                        (clicked.count() + 1) / 2
                    };
                    let taken = self
                        .base_mut()
                        .with_slot(player, idx, |s, e| s.try_remove(e, amount, i32::MAX));
                    if let Some(taken) = taken {
                        carried = taken;
                        self.base_mut().set_carried(carried.clone());
                        self.base_mut()
                            .with_slot(player, idx, |s, e| s.on_take(e, &carried));
                    }
                } else if self
                    .base_mut()
                    .with_slot(player, idx, |s, e| s.may_place(e, &carried))
                {
                    if ItemStack::is_same_item_same_components(&clicked, &carried) {
                        let amount = if action == ClickAction::Primary {
                            carried.count()
                        } else {
                            1
                        };
                        let c = std::mem::take(&mut carried);
                        carried = self
                            .base_mut()
                            .with_slot(player, idx, |s, e| s.safe_insert_amount(e, c, amount));
                    } else if carried.count()
                        <= self
                            .base_mut()
                            .with_slot(player, idx, |s, e| s.get_max_stack_size_for(e, &carried))
                    {
                        let old = std::mem::replace(&mut carried, clicked.clone());
                        self.base_mut().set_carried(carried.clone());
                        self.base_mut()
                            .with_slot(player, idx, |s, e| s.set_by_player(e, old));
                    }
                } else if ItemStack::is_same_item_same_components(&clicked, &carried) {
                    let max_amount = carried.get_max_stack_size() - carried.count();
                    let taken = self.base_mut().with_slot(player, idx, |s, e| {
                        s.try_remove(e, clicked.count(), max_amount)
                    });
                    if let Some(taken) = taken {
                        carried.grow(taken.count());
                        self.base_mut().set_carried(carried.clone());
                        self.base_mut()
                            .with_slot(player, idx, |s, e| s.on_take(e, &taken));
                    }
                }
            }
        }
        self.base_mut().set_carried(carried);
        self.base_mut()
            .with_slot(player, idx, |s, e| s.set_changed(e));
        self.flush_container_events(player);
    }

    /// SWAP with hotbar slot `button` (0..9) or the offhand (40).
    fn click_swap(&mut self, idx: usize, button: usize, player: &mut dyn MenuPlayer) {
        let source = player.inventory().get_item(button).clone();
        let target_stack = self.base_mut().with_slot(player, idx, |s, e| s.get_item(e));
        if !source.is_empty() || !target_stack.is_empty() {
            if source.is_empty() {
                if self
                    .base_mut()
                    .with_slot(player, idx, |s, e| s.may_pickup(e))
                {
                    player
                        .inventory_mut()
                        .set_item(button, target_stack.clone());
                    self.base_mut().with_slot(player, idx, |s, e| {
                        s.on_swap_craft(target_stack.count());
                        s.set_by_player(e, ItemStack::empty());
                        s.on_take(e, &target_stack);
                    });
                }
            } else if target_stack.is_empty() {
                if self
                    .base_mut()
                    .with_slot(player, idx, |s, e| s.may_place(e, &source))
                {
                    let max = self
                        .base_mut()
                        .with_slot(player, idx, |s, e| s.get_max_stack_size_for(e, &source));
                    if source.count() > max {
                        let part = player
                            .inventory_mut()
                            .get_item_mut(button)
                            .map(|s| s.split(max))
                            .unwrap_or_default();
                        self.base_mut()
                            .with_slot(player, idx, |s, e| s.set_by_player(e, part));
                    } else {
                        player.inventory_mut().set_item(button, ItemStack::empty());
                        self.base_mut()
                            .with_slot(player, idx, |s, e| s.set_by_player(e, source));
                    }
                }
            } else if self.base_mut().with_slot(player, idx, |s, e| {
                s.may_pickup(e) && s.may_place(e, &source)
            }) {
                let max = self
                    .base_mut()
                    .with_slot(player, idx, |s, e| s.get_max_stack_size_for(e, &source));
                if source.count() > max {
                    let part = player
                        .inventory_mut()
                        .get_item_mut(button)
                        .map(|s| s.split(max))
                        .unwrap_or_default();
                    let mut target_stack = target_stack;
                    self.base_mut().with_slot(player, idx, |s, e| {
                        s.set_by_player(e, part);
                        s.on_take(e, &target_stack);
                    });
                    self.flush_container_events(player);
                    let infinite = player.has_infinite_materials();
                    if !player.inventory_mut().add(&mut target_stack, infinite) {
                        player.drop(target_stack, true, Prediction::Predicted);
                    }
                } else {
                    player
                        .inventory_mut()
                        .set_item(button, target_stack.clone());
                    self.base_mut().with_slot(player, idx, |s, e| {
                        s.set_by_player(e, source);
                        s.on_take(e, &target_stack);
                    });
                }
            }
        }
        self.flush_container_events(player);
    }
}

/// `AbstractContainerMenu.dropOrPlaceInInventory`.
pub fn drop_or_place_in_inventory(player: &mut dyn MenuPlayer, mut carried: ItemStack) {
    if player.is_server_player() {
        if !player.is_removed_not_changing_dimension() && !player.has_disconnected() {
            place_item_back_in_inventory(player, &mut carried, true, Prediction::ServerOnly);
        } else {
            player.drop(carried, false, Prediction::ServerOnly);
        }
    }
}
