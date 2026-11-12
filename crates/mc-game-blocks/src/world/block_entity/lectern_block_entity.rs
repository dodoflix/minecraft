//! `LecternBlockEntity`: the book on a lectern and the page being read. It is its own `bookAccess` container (one
//! slot, `setItem` ignored, never accepts items) and `dataAccess` (`DATA_PAGE`) for the `LecternMenu`.
//!
//! Vanilla's container methods reach the level (`onBookItemRemove` -> `LecternBlock.resetBookState`, `setChanged`,
//! `setPage` -> `signalPageChange`); the container methods here only queue those effects in order, and the menu
//! resolver runs them through [`BlockEntity::container_accessed`] right after the container operation.

use linkme::distributed_slice;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, set_changed,
};
use mc_game_core::world::container::{Container, empty_stack, still_valid_block_entity};
use mc_game_core::world::inventory::{ContainerData, MenuPlayer};
use mc_game_core::world::item::ItemStack;
use mc_nbt::CompoundTag;
use mc_protocol::components::{WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT};
use std::any::Any;

pub const DATA_PAGE: usize = 0;
pub const NUM_DATA: usize = 1;
pub const SLOT_BOOK: usize = 0;
pub const NUM_SLOTS: usize = 1;

/// A level effect queued by a container/data operation (see the module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pending {
    /// `setChanged()`.
    Changed,
    /// `onBookItemRemove()`'s `LecternBlock.resetBookState(null, level, pos, state, false)`.
    ResetBookState,
    /// `LecternBlock.signalPageChange(level, pos, state)`.
    SignalPageChange,
}

pub struct LecternBlockEntity {
    base: BlockEntityData,
    book: ItemStack,
    page: i32,
    page_count: i32,
    pending: Vec<Pending>,
}

/// `getPageCount(book)`.
fn get_page_count(book: &ItemStack) -> i32 {
    if let Some(written) = book.get(&WRITTEN_BOOK_CONTENT) {
        written.pages.len() as i32
    } else if let Some(writable) = book.get(&WRITABLE_BOOK_CONTENT) {
        writable.pages.len() as i32
    } else {
        0
    }
}

impl LecternBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::LECTERN, pos, state),
            book: ItemStack::empty(),
            page: 0,
            page_count: 0,
            pending: Vec::new(),
        }
    }

    /// `getBook()`.
    pub fn get_book(&self) -> &ItemStack {
        &self.book
    }

    /// `hasBook()`.
    pub fn has_book(&self) -> bool {
        self.book.get(&WRITABLE_BOOK_CONTENT).is_some()
            || self.book.get(&WRITTEN_BOOK_CONTENT).is_some()
    }

    /// `setBook(book)` = `setBook(book, null)`: resolves a written book with the "Lectern" source, then installs it.
    pub fn set_book(&mut self, level: &mut ServerLevel, mut book: ItemStack) {
        self.resolve_book(level, &mut book);
        self.book = book;
        self.page = 0;
        self.page_count = get_page_count(&self.book);
        set_changed(level, self.base.pos, self.base.block_state);
    }

    /// `resolveBook(book, null)`: resolves the components of a written book (`WrittenBookContent.resolveForItem`) with
    /// a game-master "Lectern" source at the lectern. Vanilla only does it for a block entity that has a level; here the
    /// server must be installed (`enter_server`), which block use from a player packet guarantees, while chunk loading
    /// (no level yet in vanilla either) does not resolve.
    fn resolve_book(&self, level: &ServerLevel, book: &mut ItemStack) {
        use mc_core::phys::{Vec2, Vec3};
        use mc_game_core::commands::command_source::null_source;
        use mc_game_core::commands::command_source_stack::CommandSourceStack;
        use mc_game_core::commands::component_utils::ResolutionContext;
        use mc_game_core::server::permissions::{LevelBasedPermissionSet, PermissionSet};
        if !mc_game_core::commands::server_access::has_server() {
            return;
        }
        let permissions: PermissionSet = LevelBasedPermissionSet::GAMEMASTER.into();
        let source = CommandSourceStack::named(
            null_source(),
            Vec3::at_center_of(&self.base.pos),
            Vec2::ZERO,
            level.dimension.clone(),
            permissions,
            mc_text::Component::literal("Lectern"),
        );
        mc_game_core::world::item::resolve_for_item(book, &ResolutionContext::create(&source));
    }

    /// `getPage()`.
    pub fn get_page(&self) -> i32 {
        self.page
    }

    /// `getRedstoneSignal()`.
    pub fn get_redstone_signal(&self) -> i32 {
        let page_progress = if self.page_count > 1 {
            self.page as f32 / (self.page_count as f32 - 1.0)
        } else {
            1.0
        };
        mth::floor_f32(page_progress * 14.0) + i32::from(self.has_book())
    }

    fn on_book_item_remove(&mut self) {
        self.page = 0;
        self.page_count = 0;
        self.pending.push(Pending::ResetBookState);
    }

    /// `setPage(page)`.
    fn set_page(&mut self, page: i32) {
        let new_page = mth::clamp_i32(page, 0, self.page_count - 1);
        if new_page != self.page {
            self.page = new_page;
            self.pending.push(Pending::Changed);
            self.pending.push(Pending::SignalPageChange);
        }
    }
}

impl BlockEntity for LecternBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.book = mc_game_core::world::block_entity::read_codec(tag, "Book", &ItemStack::codec())
            .unwrap_or_default();
        self.page_count = get_page_count(&self.book);
        self.page = mth::clamp_i32(tag.get_int_or("Page", 0), 0, self.page_count - 1);
    }

    fn load_additional_in_level(
        &mut self,
        level: &mut ServerLevel,
        tag: &CompoundTag,
    ) -> Option<BlockState> {
        self.load_additional(tag);
        let mut book = std::mem::take(&mut self.book);
        self.resolve_book(level, &mut book);
        self.book = book;
        self.page_count = get_page_count(&self.book);
        self.page = mth::clamp_i32(tag.get_int_or("Page", 0), 0, self.page_count - 1);
        None
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if !self.book.is_empty() {
            if let Some(book) = self.book.to_tag().result() {
                tag.put("Book", book);
            }
            tag.put_int("Page", self.page);
        }
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }
    fn container_data(&self) -> Option<&dyn ContainerData> {
        Some(self)
    }
    fn container_data_mut(&mut self) -> Option<&mut dyn ContainerData> {
        Some(self)
    }

    /// `LecternBlockEntity` is its own `MenuProvider`.
    fn menu_provider(&self) -> Option<Box<dyn mc_game_core::world::inventory::MenuProvider>> {
        Some(crate::world::level::block::impls::lectern_block::lectern_menu_provider(self.base.pos))
    }

    fn container_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn container_accessed(&mut self, level: &mut ServerLevel) {
        use crate::world::level::block::impls::lectern_block::{
            reset_book_state, signal_page_change,
        };
        let (pos, state) = (self.base.pos, self.base.block_state);
        for effect in std::mem::take(&mut self.pending) {
            match effect {
                Pending::Changed => set_changed(level, pos, state),
                Pending::ResetBookState => reset_book_state(None, level, pos, state, false),
                Pending::SignalPageChange => signal_page_change(level, pos, state),
            }
        }
    }

    /// `preRemoveSideEffects`: a lectern with a book drops it in front of the lectern.
    fn pre_remove_side_effects(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        state: BlockState,
    ) {
        use crate::world::level::block::impls::lectern_block::drop_book;
        drop_book(level, pos, state, &self.book);
    }
}

impl Container for LecternBlockEntity {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        NUM_SLOTS
    }
    fn is_empty(&self) -> bool {
        self.book.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        if slot == SLOT_BOOK {
            &self.book
        } else {
            empty_stack()
        }
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        (slot == SLOT_BOOK).then_some(&mut self.book)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        if slot != SLOT_BOOK {
            return ItemStack::empty();
        }
        let result = self.book.split(count);
        if self.book.is_empty() {
            self.on_book_item_remove();
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        if slot != SLOT_BOOK {
            return ItemStack::empty();
        }
        let previous = std::mem::take(&mut self.book);
        self.on_book_item_remove();
        previous
    }
    /// The anonymous container ignores `setItem`.
    fn set_item(&mut self, _slot: usize, _stack: ItemStack) {}
    fn clear_content(&mut self) {}
    fn set_changed(&mut self) {
        self.pending.push(Pending::Changed);
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        still_valid_block_entity(self.base.pos, player) && self.has_book()
    }
    fn get_max_stack_size(&self) -> i32 {
        1
    }
    fn can_place_item(&self, _slot: usize, _stack: &ItemStack) -> bool {
        false
    }
}

impl ContainerData for LecternBlockEntity {
    fn get(&self, data_id: usize) -> i32 {
        if data_id == DATA_PAGE { self.page } else { 0 }
    }
    fn set(&mut self, data_id: usize, value: i32) {
        if data_id == DATA_PAGE {
            self.set_page(value);
        }
    }
    fn get_count(&self) -> usize {
        NUM_DATA
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static LECTERN: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::LECTERN,
    create: |pos, state| Box::new(LecternBlockEntity::new(pos, state)),
};
