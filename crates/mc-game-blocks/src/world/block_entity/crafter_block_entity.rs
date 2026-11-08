//! `CrafterBlockEntity` (`RandomizableContainerBlockEntity` + `CraftingContainer`): nine slots, per-slot disabled flags,
//! `triggered`, `crafting_ticks_remaining`.
//!
//! Not implemented: the crafter menu (`CrafterMenu`) does not exist yet, so `menu_provider` stays `None` and the block
//! opens nothing; `container_data` plus [`CrafterBlockEntity::set_slot_state`] / [`CrafterBlockEntity::is_slot_disabled`]
//! are what that menu reads.

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::{BlockEntityType, props};
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::base_container::BaseContainer;
use mc_game_core::world::block_entity::randomizable_container::{
    RandomizableContainerData, unpack_loot_table,
};
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, changed_log,
    deferred,
};
use mc_game_core::world::container::{Container, empty_stack, helper};
use mc_game_core::world::inventory::{ContainerData, MenuPlayer};
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::item::crafting::CraftingInput;
use mc_game_core::world::level::LevelAccessor;
use mc_nbt::CompoundTag;
use mc_protocol::components::map::DataComponentMapBuilder;
use mc_text::Component;
use std::any::Any;

pub const CONTAINER_WIDTH: usize = 3;
pub const CONTAINER_HEIGHT: usize = 3;
pub const CONTAINER_SIZE: usize = 9;
pub const SLOT_DISABLED: i32 = 1;
pub const SLOT_ENABLED: i32 = 0;
pub const DATA_TRIGGERED: usize = 9;
pub const NUM_DATA: usize = 10;

pub struct CrafterBlockEntity {
    base: BlockEntityData,
    pub container: BaseContainer,
    pub loot: RandomizableContainerData,
    items: [ItemStack; CONTAINER_SIZE],
    crafting_ticks_remaining: i32,
    /// `containerData`: the nine slot states, then `triggered`.
    data: [i32; NUM_DATA],
}

impl CrafterBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::CRAFTER, pos, state),
            container: BaseContainer::default(),
            loot: RandomizableContainerData::default(),
            items: std::array::from_fn(|_| ItemStack::empty()),
            crafting_ticks_remaining: 0,
            data: [0; NUM_DATA],
        }
    }

    /// `getDefaultName()`.
    pub fn get_default_name() -> Component {
        Component::translatable("container.crafter", Vec::new())
    }

    /// `getDisplayName()`.
    pub fn get_display_name(&self) -> Component {
        self.container.get_name(Self::get_default_name())
    }

    /// `getItems()`.
    pub fn items(&self) -> &[ItemStack; CONTAINER_SIZE] {
        &self.items
    }

    pub fn items_mut(&mut self) -> &mut [ItemStack; CONTAINER_SIZE] {
        &mut self.items
    }

    /// `setSlotState(slotId, enabled)`.
    pub fn set_slot_state(&mut self, slot_id: i32, enabled: bool) {
        if self.slot_can_be_disabled(slot_id) {
            self.data[slot_id as usize] = if enabled { SLOT_ENABLED } else { SLOT_DISABLED };
            Container::set_changed(self);
        }
    }

    /// `isSlotDisabled(slotId)`.
    pub fn is_slot_disabled(&self, slot_id: i32) -> bool {
        (0..CONTAINER_SIZE as i32).contains(&slot_id)
            && self.data[slot_id as usize] == SLOT_DISABLED
    }

    /// `smallerStackExist(baseSize, baseItem, baseSlot)`.
    fn smaller_stack_exist(&self, base_size: i32, base_item: &ItemStack, base_slot: usize) -> bool {
        (base_slot + 1..CONTAINER_SIZE).any(|i| {
            !self.is_slot_disabled(i as i32) && {
                let slot_stack = &self.items[i];
                slot_stack.is_empty()
                    || slot_stack.count() < base_size
                        && ItemStack::is_same_item_same_components(slot_stack, base_item)
            }
        })
    }

    /// `setTriggered(value)`.
    pub fn set_triggered(&mut self, value: bool) {
        self.data[DATA_TRIGGERED] = i32::from(value);
    }

    /// `isTriggered()`.
    pub fn is_triggered(&self) -> bool {
        self.data[DATA_TRIGGERED] == 1
    }

    /// `setCraftingTicksRemaining(maxCraftingTicks)`.
    pub fn set_crafting_ticks_remaining(&mut self, max_crafting_ticks: i32) {
        self.crafting_ticks_remaining = max_crafting_ticks;
    }

    /// `getRedstoneSignal()`: the slots that hold an item or are disabled.
    pub fn get_redstone_signal(&self) -> i32 {
        (0..CONTAINER_SIZE)
            .filter(|&i| !self.items[i].is_empty() || self.is_slot_disabled(i as i32))
            .count() as i32
    }

    /// `slotCanBeDisabled(slotId)`.
    fn slot_can_be_disabled(&self, slot_id: i32) -> bool {
        (0..CONTAINER_SIZE as i32).contains(&slot_id) && self.items[slot_id as usize].is_empty()
    }

    /// `asCraftInput()`: the bounding box of the filled slots.
    pub fn as_craft_input(&self) -> CraftingInput {
        CraftingInput::of_positioned(CONTAINER_WIDTH, CONTAINER_HEIGHT, &self.items).input
    }

    /// `serverTick(level, pos, state, entity)`. The own-position `setBlock` waits for the entity to be back in its chunk.
    fn tick(&mut self) {
        let crafting_ticks_remaining = self.crafting_ticks_remaining - 1;
        if crafting_ticks_remaining >= 0 {
            self.crafting_ticks_remaining = crafting_ticks_remaining;
            if crafting_ticks_remaining == 0 {
                let (pos, state) = (
                    self.base.pos,
                    self.base.block_state.set(props::CRAFTING, false),
                );
                self.base.block_state = state;
                deferred::push(move |level| {
                    level.set_block_and_update(pos, state);
                });
            }
        }
    }
}

impl Container for CrafterBlockEntity {
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
        if self.is_slot_disabled(slot as i32) {
            self.set_slot_state(slot as i32, true);
        }
        if slot < CONTAINER_SIZE {
            let max = self.get_max_stack_size_for(&stack);
            stack.limit_size(max);
            self.items[slot] = stack;
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
        mc_game_core::world::container::still_valid_block_entity(self.base.pos, player)
    }
    fn can_place_item(&self, slot: usize, _stack: &ItemStack) -> bool {
        if self.data[slot] == SLOT_DISABLED {
            return false;
        }
        let slot_stack = &self.items[slot];
        let current_stack_size = slot_stack.count();
        if current_stack_size >= slot_stack.get_max_stack_size() {
            false
        } else {
            slot_stack.is_empty() || !self.smaller_stack_exist(current_stack_size, slot_stack, slot)
        }
    }
}

/// `containerData`.
impl ContainerData for CrafterBlockEntity {
    fn get(&self, data_id: usize) -> i32 {
        self.data[data_id]
    }
    fn set(&mut self, data_id: usize, value: i32) {
        self.data[data_id] = value;
    }
    fn get_count(&self) -> usize {
        NUM_DATA
    }
}

impl BlockEntity for CrafterBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.container.load(tag);
        self.crafting_ticks_remaining = tag.get_int_or("crafting_ticks_remaining", 0);
        self.items = std::array::from_fn(|_| ItemStack::empty());
        if !self.loot.try_load_loot_table(tag) {
            helper::load_all_items(tag, &mut self.items);
        }
        self.data[..CONTAINER_SIZE].fill(SLOT_ENABLED);
        if let Some(disabled) = tag.get_int_array("disabled_slots") {
            for &slot in disabled {
                if self.slot_can_be_disabled(slot) {
                    self.data[slot as usize] = SLOT_DISABLED;
                }
            }
        }
        self.data[DATA_TRIGGERED] = tag.get_int_or("triggered", 0);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.container.save(tag);
        tag.put_int("crafting_ticks_remaining", self.crafting_ticks_remaining);
        if !self.loot.try_save_loot_table(tag)
            && let Some(items) = helper::save_all_items(&self.items, true)
        {
            tag.put("Items", items);
        }
        let disabled = (0..CONTAINER_SIZE as i32)
            .filter(|&slot| self.is_slot_disabled(slot))
            .collect();
        tag.put_int_array("disabled_slots", disabled);
        tag.put_int("triggered", self.data[DATA_TRIGGERED]);
    }

    fn apply_components_from_item_stack(&mut self, stack: &ItemStack) {
        self.container
            .apply_implicit_components(stack, &mut self.items);
        self.loot.apply_implicit_components(stack);
    }

    fn collect_implicit_components(&self, builder: &mut DataComponentMapBuilder) {
        *builder = self
            .container
            .collect_implicit_components(std::mem::take(builder), &self.items);
        if let Some(table) = &self.loot.loot_table {
            *builder = std::mem::take(builder).set(
                mc_protocol::components::CONTAINER_LOOT,
                mc_protocol::components::SeededContainerLoot {
                    loot_table: table.clone(),
                    seed: self.loot.loot_table_seed,
                },
            );
        }
    }

    fn get_custom_name(&self) -> Option<Component> {
        self.container.get_custom_name().cloned()
    }

    fn unpack_loot_table(
        &mut self,
        level: &mut ServerLevel,
        player: Option<&mut mc_game_core::world::entity::ServerPlayer>,
    ) {
        let pos = self.base.pos;
        let mut loot = std::mem::take(&mut self.loot);
        unpack_loot_table(level, pos, &mut loot, self, player);
        self.loot = loot;
    }

    /// Always `false`: `getItem`/`isEmpty` are overridden without `unpackLootTable`, so the comparator signal (the only
    /// reader of this hook) never rolls the table.
    fn has_loot_table(&self) -> bool {
        false
    }

    fn set_loot_table(&mut self, table: mc_core::ident::Identifier, seed: i64) {
        self.loot.set_loot_table(table, seed);
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, _level: &mut ServerLevel) {
        self.tick();
    }

    /// Container operations replay the recorded `setChanged()` calls right after the operation.
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

    fn container_data(&self) -> Option<&dyn ContainerData> {
        Some(self)
    }

    fn container_data_mut(&mut self) -> Option<&mut dyn ContainerData> {
        Some(self)
    }

    /// `BlockEntity.preRemoveSideEffects`: `dropContents` reads `getItem`, which this type does not unpack.
    fn pre_remove_side_effects(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        _state: BlockState,
    ) {
        mc_game_core::world::containers::drop_contents_at(level, pos, &*self);
    }
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static CRAFTER: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::CRAFTER,
    // `CrafterBlock.newBlockEntity`: `setTriggered(state.getValue(TRIGGERED))`.
    create: |pos, state| {
        let mut crafter = CrafterBlockEntity::new(pos, state);
        crafter.set_triggered(state.has_property(props::TRIGGERED) && state.get(props::TRIGGERED));
        Box::new(crafter)
    },
};
