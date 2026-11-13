//! The player's open-menu lifecycle (`ServerPlayer.containerMenu/inventoryMenu/containerCounter`, `openMenu`,
//! `closeContainer`, `doCloseContainer`, `initMenu`) and the serverbound menu handlers
//! (`ServerGamePacketListenerImpl.handleContainerClick/Close/ButtonClick/SetCreativeModeSlot`).
//!
//! The player embeds a [`PlayerMenus`]; the menu in use is *taken out* while it runs so it can receive the player
//! itself as `&mut dyn MenuPlayer`.

use super::{
    AbstractContainerMenu, ContainerListener, ContainerSynchronizer, InventoryMenu, MenuClickError,
    MenuPlayer,
};
use crate::world::item::ItemStack;
use mc_protocol::packets::play::ServerboundContainerClickPacket;

/// Creates the synchronizer/listener pair a freshly opened menu is initialised with (`initMenu`).
pub trait MenuInit {
    fn container_synchronizer(&self) -> Box<dyn ContainerSynchronizer>;
    fn container_listener(&self) -> Box<dyn ContainerListener>;
}

pub struct PlayerMenus {
    /// `inventoryMenu` (taken out while it runs).
    inventory_menu: Option<Box<dyn AbstractContainerMenu>>,
    /// `containerMenu` when it is not the inventory menu.
    container_menu: Option<Box<dyn AbstractContainerMenu>>,
    /// `containerCounter`.
    container_counter: i32,
}

impl PlayerMenus {
    pub fn new() -> Self {
        Self {
            inventory_menu: Some(Box::new(InventoryMenu::new(true))),
            container_menu: None,
            container_counter: 0,
        }
    }

    /// `containerMenu != inventoryMenu` (`hasContainerOpen`).
    pub fn has_container_open(&self) -> bool {
        self.container_menu.is_some()
    }

    /// `containerMenu.containerId`.
    pub fn current_container_id(&self) -> i32 {
        match &self.container_menu {
            Some(m) => m.base().container_id,
            None => super::CONTAINER_ID,
        }
    }

    /// `nextContainerCounter()`; returns the new counter.
    pub fn next_container_counter(&mut self) -> i32 {
        self.container_counter = self.container_counter % 100 + 1;
        self.container_counter
    }

    pub fn container_counter(&self) -> i32 {
        self.container_counter
    }

    pub fn inventory_menu(&self) -> &dyn AbstractContainerMenu {
        self.inventory_menu
            .as_deref()
            .expect("inventory menu is in use")
    }

    pub fn inventory_menu_mut(&mut self) -> &mut dyn AbstractContainerMenu {
        self.inventory_menu
            .as_deref_mut()
            .expect("inventory menu is in use")
    }

    pub fn current(&self) -> &dyn AbstractContainerMenu {
        match &self.container_menu {
            Some(m) => m.as_ref(),
            None => self.inventory_menu(),
        }
    }

    pub fn current_mut(&mut self) -> &mut dyn AbstractContainerMenu {
        match &mut self.container_menu {
            Some(m) => m.as_mut(),
            None => self
                .inventory_menu
                .as_deref_mut()
                .expect("inventory menu is in use"),
        }
    }

    /// Takes the current menu out (`None` slot means the inventory menu), for running it with the player.
    fn take_current(&mut self) -> (Box<dyn AbstractContainerMenu>, bool) {
        match self.container_menu.take() {
            Some(m) => (m, false),
            None => (
                self.inventory_menu
                    .take()
                    .expect("inventory menu is in use"),
                true,
            ),
        }
    }

    fn put_current(&mut self, menu: Box<dyn AbstractContainerMenu>, is_inventory: bool) {
        if is_inventory {
            self.inventory_menu = Some(menu);
        } else {
            self.container_menu = Some(menu);
        }
    }

    /// Runs `f` on the current menu with the player (the menu is detached from `PlayerMenus` meanwhile: `get` returns
    /// `None` through `menus_of`, so `player_of` must expose the `PlayerMenus` field).
    pub fn with_current<P: MenuPlayer + ?Sized, R>(
        player: &mut P,
        menus_of: impl Fn(&mut P) -> &mut PlayerMenus,
        f: impl FnOnce(&mut dyn AbstractContainerMenu, &mut P) -> R,
    ) -> R {
        let (mut menu, is_inv) = menus_of(player).take_current();
        let r = f(menu.as_mut(), player);
        menus_of(player).put_current(menu, is_inv);
        r
    }

    /// `initMenu(menu)` + `containerMenu = menu` for a menu already built with `next_container_counter()`'s id.
    /// (The caller sends `ClientboundOpenScreenPacket` first, like `ServerPlayer.openMenu`.)
    pub fn set_container_menu(
        &mut self,
        mut menu: Box<dyn AbstractContainerMenu>,
        init: &dyn MenuInit,
        player: &mut dyn MenuPlayer,
    ) {
        menu.add_slot_listener(init.container_listener(), player);
        menu.base_mut()
            .set_synchronizer(init.container_synchronizer(), player);
        self.container_menu = Some(menu);
    }

    /// `containerMenu = menu` without a connection to synchronise with (connection-less players).
    pub fn put_container_menu(&mut self, menu: Box<dyn AbstractContainerMenu>) {
        self.container_menu = Some(menu);
    }

    /// `initInventoryMenu()`.
    pub fn init_inventory_menu(&mut self, init: &dyn MenuInit, player: &mut dyn MenuPlayer) {
        let menu = self.inventory_menu_mut();
        menu.add_slot_listener(init.container_listener(), player);
        menu.base_mut()
            .set_synchronizer(init.container_synchronizer(), player);
    }

    /// `doCloseContainer()`: `containerMenu.removed(player)`; `inventoryMenu.transferState(containerMenu)`;
    /// `containerMenu = inventoryMenu`. A no-op beyond `removed` when the inventory menu is the current one
    /// (vanilla calls `removed` on it too, which drops the carried item and clears the crafting grid).
    pub fn do_close_container(&mut self, player: &mut dyn MenuPlayer) {
        match self.container_menu.take() {
            Some(mut menu) => {
                menu.removed(player);
                let inv = self
                    .inventory_menu
                    .as_deref_mut()
                    .expect("inventory menu is in use");
                inv.base_mut().transfer_state(menu.base());
            }
            None => {
                let inv = self
                    .inventory_menu
                    .as_deref_mut()
                    .expect("inventory menu is in use");
                inv.removed(player);
                // transferState(self) is a no-op on identical remote state.
            }
        }
    }
}

impl Default for PlayerMenus {
    fn default() -> Self {
        Self::new()
    }
}

/// `handleContainerClick`'s menu part (the caller has already done `resetLastActionTime`). `is_dead_or_dying` =
/// `player.isDeadOrDying()`. Returns `Err` for the index-out-of-bounds exception vanilla would throw from `clicked`.
pub fn handle_container_click(
    menu: &mut dyn AbstractContainerMenu,
    player: &mut dyn MenuPlayer,
    packet: &ServerboundContainerClickPacket,
    is_dead_or_dying: bool,
) -> Result<(), MenuClickError> {
    if menu.base().container_id != packet.container_id {
        return Ok(());
    }
    let slot_index = packet.slot_num as i32;
    if player.is_spectator() || is_dead_or_dying {
        menu.base_mut().send_all_data_to_remote(player);
    } else if !menu.still_valid(player) {
        log::debug!("Player {} interacted with invalid menu", player.entity_id());
    } else if !menu.base().is_valid_slot_index(slot_index) {
        log::debug!(
            "Player {} clicked invalid slot index: {}, available slots: {}",
            player.entity_id(),
            slot_index,
            menu.base().slots.len()
        );
    } else {
        let full_resync_needed = packet.state_id != menu.base().get_state_id();
        menu.base_mut().suppress_remote_updates();
        // `clicked` throwing leaves `suppressRemoteUpdates` on and skips the hashed slot/carried bookkeeping
        menu.clicked(
            slot_index,
            packet.button_num as i32,
            packet.container_input,
            player,
        )?;
        for (slot, stack) in &packet.changed_slots {
            menu.base_mut()
                .set_remote_slot_unsafe(*slot as i32, stack.clone());
        }
        menu.base_mut()
            .set_remote_carried(packet.carried_item.clone());
        menu.base_mut().resume_remote_updates();
        if full_resync_needed {
            menu.broadcast_full_state(player);
        } else {
            menu.broadcast_changes(player);
        }
    }
    Ok(())
}

/// `handleContainerButtonClick`.
pub fn handle_container_button_click(
    menu: &mut dyn AbstractContainerMenu,
    player: &mut dyn MenuPlayer,
    container_id: i32,
    button_id: i32,
) {
    if menu.base().container_id == container_id && !player.is_spectator() {
        if !menu.still_valid(player) {
            log::debug!("Player {} interacted with invalid menu", player.entity_id());
        } else if menu.click_menu_button(player, button_id) {
            menu.broadcast_changes(player);
        }
    }
}

/// `handleContainerSlotStateChanged`: only the crafter menu acts on it.
pub fn handle_container_slot_state_changed(
    menu: &mut dyn AbstractContainerMenu,
    player: &mut dyn MenuPlayer,
    container_id: i32,
    slot_id: i32,
    new_state: bool,
) {
    if !player.is_spectator() && container_id == menu.base().container_id {
        menu.set_container_slot_state(player, slot_id, new_state);
    }
}

/// `handleSetCreativeModeSlot` for a valid `slot_num` in the inventory menu (1..=45) or a drop (negative).
/// `drop_throttle_ok` is `dropSpamThrottler.isUnderThreshold()`; the caller increments the throttler when this
/// returns `CreativeSlotResult::Dropped`.
#[derive(Debug, PartialEq, Eq)]
pub enum CreativeSlotResult {
    /// Not in creative / item disabled / invalid.
    Ignored,
    Set,
    Dropped,
    /// Drop requested but throttled (`"was dropping items too fast in creative mode, ignoring."`).
    Throttled,
}

pub fn handle_set_creative_mode_slot(
    inventory_menu: &mut dyn AbstractContainerMenu,
    player: &mut dyn MenuPlayer,
    slot_num: i32,
    stack: ItemStack,
    drop_throttle_ok: bool,
) -> CreativeSlotResult {
    if !player.has_infinite_materials() {
        return CreativeSlotResult::Ignored;
    }
    let drop = slot_num < 0;
    let valid_slot = (1..=45).contains(&slot_num);
    let valid_data = stack.is_empty() || stack.count() <= stack.get_max_stack_size();
    if valid_slot && valid_data {
        let idx = slot_num as usize;
        let base = inventory_menu.base_mut();
        base.with_slot(player, idx, |s, e| s.set_by_player(e, stack.clone()));
        base.set_remote_slot(idx, &stack);
        inventory_menu.flush_container_events(player);
        inventory_menu.broadcast_changes(player);
        CreativeSlotResult::Set
    } else if drop && valid_data {
        if drop_throttle_ok {
            player.drop(stack, true, super::Prediction::Predicted);
            CreativeSlotResult::Dropped
        } else {
            CreativeSlotResult::Throttled
        }
    } else {
        CreativeSlotResult::Ignored
    }
}

/// `ServerGamePacketListenerImpl.tryPickItem(stack)` (pick block/entity): selects or fetches `stack` in the hotbar and
/// returns the new selected slot; the caller then sends `ClientboundSetHeldSlotPacket(slot)` and runs
/// `inventoryMenu.broadcastChanges()`. The `isItemEnabled(enabledFeatures)` check is the caller's.
pub fn try_pick_item(player: &mut dyn MenuPlayer, stack: ItemStack) -> i32 {
    let infinite = player.has_infinite_materials();
    let inventory = player.inventory_mut();
    let existing = inventory.find_slot_matching_item(&stack);
    if existing != -1 {
        if crate::world::entity::player::inventory::is_hotbar_slot(existing) {
            let _ = inventory.set_selected_slot(existing);
        } else {
            inventory.pick_slot(existing as usize);
        }
    } else if infinite {
        inventory.add_and_pick_item(stack);
    }
    inventory.get_selected_slot()
}
