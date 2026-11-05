//! `ChiseledBookShelfBlockEntity` (`ListBackedContainer` of 6 books, one per slot).

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use crate::server::level::ServerLevel;
use crate::world::container::{Container, helper};
use crate::world::item::ItemStack;
use crate::world::level::block::BlockStateExt;
use crate::world::level::gameevent::GameEventContext;
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, GameEvent, Item, props};
use mc_nbt::CompoundTag;
use mc_protocol::components::{CONTAINER, ItemContainerContents, map::DataComponentMapBuilder};

pub const MAX_BOOKS_IN_STORAGE: usize = 6;

/// `ChiseledBookShelfBlock.SLOT_OCCUPIED_PROPERTIES`.
pub fn slot_occupied_property(slot: usize) -> mc_data::props::BooleanProperty {
    [props::SLOT_0_OCCUPIED, props::SLOT_1_OCCUPIED, props::SLOT_2_OCCUPIED, props::SLOT_3_OCCUPIED, props::SLOT_4_OCCUPIED, props::SLOT_5_OCCUPIED][slot]
}

/// `itemStack.is(ItemTags.BOOKSHELF_BOOKS)`.
pub fn is_bookshelf_book(item: Item) -> bool {
    mc_data::tags::get::<Item>("bookshelf_books").is_some_and(|t| t.contains(item))
}

pub struct ChiseledBookShelfBlockEntity {
    base: BlockEntityData,
    items: Vec<ItemStack>,
    last_interacted_slot: i32,
    /// `updateState(slot)` requested by a level-less container operation.
    pending_slot: Option<usize>,
}

impl ChiseledBookShelfBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::CHISELED_BOOKSHELF, pos, state), items: vec![ItemStack::empty(); MAX_BOOKS_IN_STORAGE], last_interacted_slot: -1, pending_slot: None }
    }

    pub fn get_last_interacted_slot(&self) -> i32 {
        self.last_interacted_slot
    }

    pub fn accepts_item_type(stack: &ItemStack) -> bool {
        is_bookshelf_book(stack.item())
    }

    /// `updateState(interactedSlot)`.
    pub fn update_state(&mut self, level: &mut ServerLevel, interacted_slot: usize) {
        if interacted_slot < MAX_BOOKS_IN_STORAGE {
            self.last_interacted_slot = interacted_slot as i32;
            let mut updated = self.base.block_state;
            for slot in 0..MAX_BOOKS_IN_STORAGE {
                updated = updated.set(slot_occupied_property(slot), !self.items[slot].is_empty());
            }
            crate::world::level::LevelAccessor::set_block_and_update(level, self.base.pos, updated);
            self.base.block_state = updated;
            level.game_event_at_block(GameEvent::BLOCK_CHANGE, self.base.pos, &GameEventContext::of_state(Some(updated)));
        } else {
            log::error!("Expected slot 0-5, got {interacted_slot}");
        }
    }

    /// `removeItem(slot, count)` with the level at hand.
    pub fn remove_item_with_level(&mut self, level: &mut ServerLevel, slot: usize) -> ItemStack {
        let retrieved = self.items.get_mut(slot).map(std::mem::take).unwrap_or_default();
        if !retrieved.is_empty() {
            self.update_state(level, slot);
        }
        retrieved
    }

    /// `setItem(slot, stack)` with the level at hand.
    pub fn set_item_with_level(&mut self, level: &mut ServerLevel, slot: usize, stack: ItemStack) {
        if Self::accepts_item_type(&stack) {
            self.items[slot] = stack;
            self.update_state(level, slot);
        } else if stack.is_empty() {
            self.remove_item_with_level(level, slot);
        }
    }
}

impl Container for ChiseledBookShelfBlockEntity {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.items.len()
    }
    fn is_empty(&self) -> bool {
        self.items.iter().all(|s| s.is_empty())
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.items.get(slot).unwrap_or_else(|| crate::world::container::empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.items.get_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, _count: i32) -> ItemStack {
        let retrieved = self.items.get_mut(slot).map(std::mem::take).unwrap_or_default();
        if !retrieved.is_empty() {
            self.pending_slot = Some(slot);
        }
        retrieved
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        helper::remove_item(&mut self.items, slot, 1)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if Self::accepts_item_type(&stack) {
            if slot < self.items.len() {
                self.items[slot] = stack;
                self.pending_slot = Some(slot);
            }
        } else if stack.is_empty() {
            self.remove_item(slot, self.get_max_stack_size());
        }
    }
    fn clear_content(&mut self) {
        self.items.iter_mut().for_each(|s| *s = ItemStack::empty());
    }
    fn set_changed(&mut self) {
        self.base.changed = true;
    }
    fn still_valid(&self, player: &dyn crate::world::inventory::MenuPlayer) -> bool {
        crate::world::container::still_valid_block_entity(self.base.pos, player)
    }
    fn get_max_stack_size(&self) -> i32 {
        1
    }
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        Self::accepts_item_type(stack) && (self.get_item(slot).is_empty() || self.get_item(slot).count() < self.get_max_stack_size_for(stack))
    }
    fn can_take_item(&self, into: &dyn Container, _slot: usize, stack: &ItemStack) -> bool {
        into.has_any_matching(&|to| {
            to.is_empty() || (ItemStack::is_same_item_same_components(stack, to) && to.count() + stack.count() <= into.get_max_stack_size_for(to))
        })
    }
}

impl BlockEntity for ChiseledBookShelfBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.items = vec![ItemStack::empty(); MAX_BOOKS_IN_STORAGE];
        helper::load_all_items(tag, &mut self.items);
        self.last_interacted_slot = tag.get_int_or("last_interacted_slot", -1);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        if let Some(list) = helper::save_all_items(&self.items, true) {
            tag.put("Items", mc_nbt::Tag::List(list));
        }
        tag.put_int("last_interacted_slot", self.last_interacted_slot);
    }

    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        match stack.get(&CONTAINER) {
            Some(c) => c.copy_into(&mut self.items),
            None => ItemContainerContents::default().copy_into(&mut self.items),
        }
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = std::mem::take(builder).set(CONTAINER, ItemContainerContents::from_item_stacks(&self.items));
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }

    fn container_pending(&self) -> bool {
        self.base.changed || self.pending_slot.is_some()
    }

    fn container_accessed(&mut self, level: &mut ServerLevel) {
        if std::mem::take(&mut self.base.changed) {
            super::set_changed(level, self.base.pos, self.base.block_state);
        }
        if let Some(slot) = self.pending_slot.take() {
            self.update_state(level, slot);
        }
    }

    fn pre_remove_side_effects(&mut self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState) {
        crate::world::containers::drop_contents_list(level, pos, &self.items);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CHISELED_BOOKSHELF: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::CHISELED_BOOKSHELF, create: |pos, state| Box::new(ChiseledBookShelfBlockEntity::new(pos, state)) };
