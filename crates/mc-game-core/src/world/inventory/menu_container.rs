//! `new SimpleContainer(n) { setChanged() { super.setChanged(); menu.slotsChanged(this); } }`: the anonymous
//! `SimpleContainer` subclasses of the loom, stonecutter, cartography table, anvil, grindstone, ... menus.
//!
//! The menu owns the container (`MenuBase::locals`), so the callback is queued: every vanilla `setChanged()` call
//! (made by `setItem`, a non-empty `removeItem`, `clearContent` and `Slot.setChanged`) adds one notification that the
//! menu replays after each slot operation (`AbstractContainerMenu::flush_container_events`).

use super::MenuPlayer;
use crate::world::container::{Container, SimpleContainer};
use crate::world::item::ItemStack;
use std::any::Any;

pub struct MenuSimpleContainer {
    inner: SimpleContainer,
    pending: u32,
}

impl MenuSimpleContainer {
    pub fn new(size: usize) -> Self {
        Self {
            inner: SimpleContainer::new(size),
            pending: 0,
        }
    }
}

impl Container for MenuSimpleContainer {
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
            self.pending += 1;
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.inner.remove_item_no_update(slot)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        self.inner.set_item(slot, stack);
        self.pending += 1;
    }
    fn clear_content(&mut self) {
        self.inner.clear_content();
        self.pending += 1;
    }
    fn set_changed(&mut self) {
        self.pending += 1;
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.inner.still_valid(player)
    }
    fn get_max_stack_size(&self) -> i32 {
        self.inner.get_max_stack_size()
    }
    fn take_menu_notifications(&mut self) -> u32 {
        std::mem::take(&mut self.pending)
    }
}

/// The shared tail of every `quickMoveStack`: `if (stack.isEmpty()) slot.setByPlayer(EMPTY) else slot.setChanged();`,
/// `if (stack.getCount() == clicked.getCount()) return EMPTY;`, `slot.onTake(player, stack)`. `stack` is the detached
/// stack the move worked on; returns `clicked` (or EMPTY when nothing moved).
pub fn finish_quick_move(
    menu: &mut dyn super::AbstractContainerMenu,
    player: &mut dyn MenuPlayer,
    slot_index: usize,
    stack: ItemStack,
    clicked: &ItemStack,
) -> ItemStack {
    if stack.is_empty() {
        menu.base_mut().with_slot(player, slot_index, |s, e| {
            s.set_by_player_with(e, ItemStack::empty(), clicked)
        });
    } else {
        let rest = stack.clone();
        menu.base_mut().with_slot(player, slot_index, |s, e| {
            s.with_item_mut(e, |t| *t = rest);
            s.set_changed(e);
        });
    }
    if stack.count() == clicked.count() {
        return ItemStack::empty();
    }
    menu.base_mut()
        .with_slot(player, slot_index, |s, e| s.on_take(e, &stack));
    menu.flush_container_events(player);
    clicked.clone()
}
