//! `DispenserBlockEntity` (the dropper is the same class with another type/name, see `dropper_block_entity`).

use super::base_container::BaseContainer;
use super::randomizable_container::{RandomizableContainerData, unpack_loot_table};
use super::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, changed_log,
};
use crate::server::level::ServerLevel;
use crate::world::container::{Container, empty_stack, helper};
use crate::world::entity::Entity;
use crate::world::inventory::MenuPlayer;
use crate::world::item::ItemStack;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use std::any::Any;

pub const CONTAINER_SIZE: usize = 9;

pub struct DispenserBlockEntity {
    base: BlockEntityData,
    items: Vec<ItemStack>,
    pub loot: RandomizableContainerData,
    pub container: BaseContainer,
}

impl DispenserBlockEntity {
    /// `DispenserBlockEntity(type, pos, state)` (`DISPENSER` or, for droppers, `DROPPER`).
    pub fn new(ty: BlockEntityType, pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(ty, pos, state),
            items: vec![ItemStack::empty(); CONTAINER_SIZE],
            loot: Default::default(),
            container: Default::default(),
        }
    }

    /// `getDefaultName()`: the translation key of the title.
    pub fn default_name_key(&self) -> &'static str {
        if self.base.ty == BlockEntityType::DROPPER {
            "container.dropper"
        } else {
            "container.dispenser"
        }
    }

    /// `getRandomSlot(random)`: reservoir pick over the non-empty slots (one `nextInt` per non-empty slot). The caller
    /// unpacks the loot table first.
    pub fn get_random_slot(&self, random: &mut dyn RandomSource) -> i32 {
        let mut replace_slot = -1;
        let mut replace_odds = 1;
        for (i, item) in self.items.iter().enumerate() {
            if !item.is_empty() {
                let bound = replace_odds;
                replace_odds += 1;
                if random.next_int_bounded(bound) == 0 {
                    replace_slot = i as i32;
                }
            }
        }
        replace_slot
    }

    /// `insertItem(itemStack)`: merges into / fills slots in order, returns what did not fit.
    pub fn insert_item(&mut self, mut stack: ItemStack) -> ItemStack {
        let max_stack_size = self.get_max_stack_size_for(&stack);
        for i in 0..self.items.len() {
            let target_empty = self.items[i].is_empty();
            if target_empty || ItemStack::is_same_item_same_components(&stack, &self.items[i]) {
                let transfer_count = stack.count().min(max_stack_size - self.items[i].count());
                if transfer_count > 0 {
                    if target_empty {
                        let split = stack.split(transfer_count);
                        self.set_item(i, split);
                    } else {
                        stack.shrink(transfer_count);
                        self.items[i].grow(transfer_count);
                    }
                }
                if stack.is_empty() {
                    break;
                }
            }
        }
        stack
    }

    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }
}

impl BlockEntity for DispenserBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.container.load(tag);
        self.items = vec![ItemStack::empty(); CONTAINER_SIZE];
        if !self.loot.try_load_loot_table(tag) {
            helper::load_all_items(tag, &mut self.items);
        }
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.container.save(tag);
        if !self.loot.try_save_loot_table(tag)
            && let Some(list) = helper::save_all_items(&self.items, false)
        {
            tag.put("Items", list);
        }
    }

    fn unpack_loot_table(&mut self, level: &ServerLevel, player: Option<&dyn Entity>) {
        let pos = self.base.pos;
        let mut loot = std::mem::take(&mut self.loot);
        unpack_loot_table(level, pos, &mut loot, self, player);
        self.loot = loot;
    }

    fn set_loot_table(&mut self, table: mc_core::ident::Identifier, seed: i64) {
        self.loot.set_loot_table(table, seed);
    }

    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.container
            .apply_implicit_components(stack, &mut self.items);
        self.loot.apply_implicit_components(stack);
    }

    /// Menu operations (`ContainerKey::BlockEntity`) replay the recorded `setChanged()` calls right after the operation.
    fn container_pending(&self) -> bool {
        changed_log::has_pending()
    }
    fn container_accessed(&mut self, level: &mut ServerLevel) {
        changed_log::flush(level);
    }

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self)
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
    }
}

impl Container for DispenserBlockEntity {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        CONTAINER_SIZE
    }
    fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.items.get(slot).unwrap_or_else(|| empty_stack())
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.items.get_mut(slot)
    }
    /// `BaseContainerBlockEntity.removeItem`: `setChanged` when something was removed.
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        let result = helper::remove_item(&mut self.items, slot, count);
        if !result.is_empty() {
            self.set_changed();
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        helper::take_item(&mut self.items, slot)
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let max = self.get_max_stack_size_for(&stack);
        stack.limit_size(max);
        if let Some(s) = self.items.get_mut(slot) {
            *s = stack;
        }
        self.set_changed();
    }
    fn clear_content(&mut self) {
        self.items.fill(ItemStack::empty());
    }
    fn set_changed(&mut self) {
        changed_log::push(self.base.pos);
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        crate::world::container::still_valid_block_entity(self.base.pos, player)
    }
}

#[linkme::distributed_slice(BLOCK_ENTITY_TYPES)]
static DISPENSER: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::DISPENSER,
    create: |pos, state| {
        Box::new(DispenserBlockEntity::new(
            BlockEntityType::DISPENSER,
            pos,
            state,
        ))
    },
};
