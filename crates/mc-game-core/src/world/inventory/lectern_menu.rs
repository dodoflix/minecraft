//! `LecternMenu`: the book slot and the current page (the lectern block entity's `ContainerData`).

use super::{
    AbstractContainerMenu, ContainerKey, ContainerRef, MenuBase, MenuPlayer, Prediction, Slot,
    with_player_container,
};
use crate::world::item::ItemStack;
use mc_core::pos::BlockPos;
use mc_data::MenuType;
use std::any::Any;

pub const BUTTON_PREV_PAGE: i32 = 1;
pub const BUTTON_NEXT_PAGE: i32 = 2;
pub const BUTTON_TAKE_BOOK: i32 = 3;
pub const BUTTON_PAGE_JUMP_RANGE_START: i32 = 100;

pub struct LecternMenu {
    base: MenuBase,
    pos: BlockPos,
    container: ContainerRef,
}

impl LecternMenu {
    /// `LecternMenu(containerId, lectern, lecternData)` over the lectern block entity at `pos`.
    /// (`Slot.setChanged` -> `slotsChanged` only triggers a `broadcastChanges`, which every click does anyway.)
    pub fn new(container_id: i32, player: &mut dyn MenuPlayer, pos: BlockPos) -> Self {
        let mut base = MenuBase::new(Some(MenuType::LECTERN), container_id);
        base.add_slot(Slot::new(ContainerKey::BlockEntity(pos), 0, 0, 0));
        base.add_block_entity_data_slots(pos, 1);
        let container = ContainerRef::block_entity(pos, 1);
        container.start_open(player);
        Self {
            base,
            pos,
            container,
        }
    }

    /// `getPage()`.
    pub fn get_page(&self, player: &dyn MenuPlayer) -> i32 {
        use crate::world::level::BlockGetter;
        player
            .level_ref()
            .and_then(|l| l.get_block_entity(self.pos))
            .and_then(|be| be.container_data())
            .map_or(0, |d| d.get(0))
    }

    /// `getBook()`.
    pub fn get_book(&self, player: &dyn MenuPlayer) -> ItemStack {
        use crate::world::level::BlockGetter;
        player
            .level_ref()
            .and_then(|l| l.get_block_entity(self.pos))
            .and_then(|be| be.as_container())
            .map_or_else(ItemStack::empty, |c| c.get_item(0).clone())
    }

    /// `setData(0, page)`: through the lectern's `dataAccess` (`setPage` clamps and signals the page change), then
    /// `broadcastChanges()`.
    fn set_page(&mut self, player: &mut dyn MenuPlayer, page: i32) {
        let pos = self.pos;
        if let Some(level) = player.level() {
            level.with_block_entity(pos, |level, be| {
                if let Some(data) = be.container_data_mut() {
                    data.set(0, page);
                }
                if be.container_pending() {
                    be.container_accessed(level);
                }
            });
        }
        self.broadcast_changes(player);
    }
}

impl AbstractContainerMenu for LecternMenu {
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

    fn click_menu_button(&mut self, player: &mut dyn MenuPlayer, button_id: i32) -> bool {
        if button_id >= BUTTON_PAGE_JUMP_RANGE_START {
            self.set_page(player, button_id - BUTTON_PAGE_JUMP_RANGE_START);
            return true;
        }
        match button_id {
            BUTTON_PREV_PAGE => {
                let page = self.get_page(player);
                self.set_page(player, page - 1);
                true
            }
            BUTTON_NEXT_PAGE => {
                let page = self.get_page(player);
                self.set_page(player, page + 1);
                true
            }
            BUTTON_TAKE_BOOK => {
                if !player.may_build() {
                    return false;
                }
                let key = ContainerKey::BlockEntity(self.pos);
                let mut book = with_player_container(player, &key, |c| c.remove_item_no_update(0));
                with_player_container(player, &key, |c| c.set_changed());
                let infinite = player.has_infinite_materials();
                if !player.inventory_mut().add(&mut book, infinite) {
                    player.drop(book, false, Prediction::Predicted);
                }
                true
            }
            _ => false,
        }
    }

    fn quick_move_stack(&mut self, _player: &mut dyn MenuPlayer, _slot_index: usize) -> ItemStack {
        ItemStack::empty()
    }

    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.container.still_valid(player)
    }
}
