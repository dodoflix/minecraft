//! `PlayerEnderChestContainer`: the player's 27-slot ender chest, linked to the ender chest block entity it was last
//! opened through (`setActiveChest`): that block entity is notified on open/close and decides `stillValid`.

use super::MenuPlayer;
use crate::world::container::{Container, SimpleContainer};
use crate::world::entity::EntityId;
use crate::world::item::ItemStack;
use mc_core::pos::BlockPos;
use std::any::Any;
use std::ops::{Deref, DerefMut};

pub struct PlayerEnderChestContainer {
    inner: SimpleContainer,
    /// `activeChest` (an `EnderChestBlockEntity`), by position.
    active_chest: Option<BlockPos>,
}

impl PlayerEnderChestContainer {
    pub fn new() -> Self {
        Self { inner: SimpleContainer::new(27), active_chest: None }
    }

    /// `setActiveChest(chest)`.
    pub fn set_active_chest(&mut self, pos: BlockPos) {
        self.active_chest = Some(pos);
    }

    /// `isActiveChest(chest)`.
    pub fn is_active_chest(&self, pos: BlockPos) -> bool {
        self.active_chest == Some(pos)
    }

    pub fn active_chest(&self) -> Option<BlockPos> {
        self.active_chest
    }

    /// The part of `stopOpen` after the block entity was notified: `activeChest = null`.
    pub fn clear_active_chest(&mut self) {
        self.active_chest = None;
    }

    /// `fromSlots`: every slot cleared, then the listed ones set (`isValidInContainer` = index in range).
    pub fn from_slots(&mut self, list: impl IntoIterator<Item = (usize, ItemStack)>) {
        for i in 0..self.inner.get_container_size() {
            self.inner.set_item(i, ItemStack::empty());
        }
        for (slot, stack) in list {
            if slot < self.inner.get_container_size() {
                self.inner.set_item(slot, stack);
            }
        }
    }
}

impl Default for PlayerEnderChestContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for PlayerEnderChestContainer {
    type Target = SimpleContainer;
    fn deref(&self) -> &SimpleContainer {
        &self.inner
    }
}

impl DerefMut for PlayerEnderChestContainer {
    fn deref_mut(&mut self) -> &mut SimpleContainer {
        &mut self.inner
    }
}

impl Container for PlayerEnderChestContainer {
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
        self.inner.remove_item(slot, count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.inner.remove_item_no_update(slot)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        self.inner.set_item(slot, stack);
    }
    fn clear_content(&mut self) {
        self.inner.clear_content();
    }
    fn set_changed(&mut self) {
        self.inner.set_changed();
    }
    /// `activeChest != null && !activeChest.stillValid(player) ? false : super.stillValid(player)`.
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        match self.active_chest {
            Some(pos) => crate::world::container::still_valid_block_entity(pos, player),
            None => true,
        }
    }
    fn start_open(&mut self, _user: EntityId) {}
    fn stop_open(&mut self, _user: EntityId) {}
}
