//! `net.minecraft.world.Container`, `SimpleContainer`, `ContainerHelper`, `ItemStackWithSlot`.
//!
//! Java hands out the stored `ItemStack` object and callers mutate it in place (`stack.grow(1)`); the Rust
//! `get_item` returns `&ItemStack` and [`Container::get_item_mut`] the in-place `&mut` (no `setItem` side effects,
//! exactly like mutating the Java object). Out-of-range reads return an empty stack like vanilla.

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::inventory::MenuPlayer;
use crate::world::item::ItemStack;
use crate::world::item::{ItemStackExt, load_item_stack};
use mc_core::direction::Direction;
use mc_data::Item;
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::any::Any;
use std::sync::LazyLock;

static EMPTY_STACK: LazyLock<ItemStack> = LazyLock::new(ItemStack::empty);

/// A shared immutable empty stack (what `ItemStack.EMPTY` is for read-only returns).
pub fn empty_stack() -> &'static ItemStack {
    &EMPTY_STACK
}

/// `Container.stillValidBlockEntity(blockEntity, player)` (distance buffer 4.0) for the block entity at `pos`: it must
/// still be the live block entity of the player's level and in interaction range.
pub fn still_valid_block_entity(pos: mc_core::pos::BlockPos, player: &dyn MenuPlayer) -> bool {
    still_valid_block_entity_range(pos, player, 4.0)
}

/// `stillValidBlockEntity(blockEntity, player, distanceBuffer)`.
pub fn still_valid_block_entity_range(
    pos: mc_core::pos::BlockPos,
    player: &dyn MenuPlayer,
    distance_buffer: f64,
) -> bool {
    use crate::world::level::BlockGetter;
    player
        .level_ref()
        .and_then(|l| l.get_block_entity(pos))
        .is_some_and(|be| !be.is_removed())
        && player.is_within_block_interaction_range(pos, distance_buffer)
}

/// `Container` (+ `Clearable`).
pub trait Container: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn get_container_size(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn get_item(&self, slot: usize) -> &ItemStack;
    /// The stored stack for in-place mutation (`None` when `slot` is out of range). Must not run `setItem` hooks.
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack>;
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack;
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack;
    fn set_item(&mut self, slot: usize, stack: ItemStack);
    fn clear_content(&mut self);
    fn set_changed(&mut self);
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool;

    fn get_max_stack_size(&self) -> i32 {
        99
    }
    fn get_max_stack_size_for(&self, stack: &ItemStack) -> i32 {
        self.get_max_stack_size().min(stack.get_max_stack_size())
    }
    /// `startOpen(containerUser)` (the user is identified by entity id).
    fn start_open(&mut self, _user: EntityId) {}
    fn stop_open(&mut self, _user: EntityId) {}
    fn get_entities_with_container_open(&self) -> Vec<EntityId> {
        Vec::new()
    }
    fn can_place_item(&self, _slot: usize, _stack: &ItemStack) -> bool {
        true
    }
    /// `canPlaceItem(slot, stack)` of a container whose answer reads `this.level` (the brewing stand's recipes).
    fn can_place_item_in_level(
        &self,
        _level: &ServerLevel,
        slot: usize,
        stack: &ItemStack,
    ) -> bool {
        self.can_place_item(slot, stack)
    }
    fn can_take_item(&self, _into: &dyn Container, _slot: usize, _stack: &ItemStack) -> bool {
        true
    }
    /// Number of `slotsChanged(container)` notifications this container owes its menu since the last call. Containers
    /// whose vanilla `setItem`/`removeItem`/`setChanged` call back into the owning menu (crafting grids, anvil inputs,
    /// merchant slots) count them here; the menu replays them after every slot operation (`flush_container_events`).
    fn take_menu_notifications(&mut self) -> u32 {
        0
    }

    fn count_item(&self, item: Item) -> i32 {
        (0..self.get_container_size())
            .map(|i| self.get_item(i))
            .filter(|s| s.item() == item)
            .map(ItemStack::count)
            .sum()
    }
    fn has_any_of(&self, items: &[Item]) -> bool {
        self.has_any_matching(&|s| !s.is_empty() && items.contains(&s.item()))
    }
    fn has_any_matching(&self, predicate: &dyn Fn(&ItemStack) -> bool) -> bool {
        (0..self.get_container_size()).any(|i| predicate(self.get_item(i)))
    }
    /// `this instanceof WorldlyContainer`.
    fn as_worldly(&self) -> Option<&dyn WorldlyContainer> {
        None
    }
}

/// `WorldlyContainer`: a container with per-face slot access (furnaces, brewing stands, shulker boxes, composters...).
/// `can_place_item_through_face` gets the level because vanilla's implementations read `this.level` (brewing recipes).
pub trait WorldlyContainer: Container {
    /// `getSlotsForFace(direction)`.
    fn get_slots_for_face(&self, direction: Direction) -> &'static [usize];
    /// `canPlaceItemThroughFace(slot, stack, direction)`.
    fn can_place_item_through_face(
        &self,
        level: &ServerLevel,
        slot: usize,
        stack: &ItemStack,
        direction: Option<Direction>,
    ) -> bool;
    /// `canTakeItemThroughFace(slot, stack, direction)`.
    fn can_take_item_through_face(
        &self,
        slot: usize,
        stack: &ItemStack,
        direction: Direction,
    ) -> bool;
}

/// A block entity (that is a `Container`) that was taken out of its chunk, usable as an owned `Container`.
pub struct BlockEntityContainer(pub Box<dyn crate::world::block_entity::BlockEntity>);

impl BlockEntityContainer {
    fn inner(&self) -> &dyn Container {
        self.0
            .as_container()
            .expect("BlockEntityContainer wraps a container block entity")
    }
    fn inner_mut(&mut self) -> &mut dyn Container {
        self.0
            .as_container_mut()
            .expect("BlockEntityContainer wraps a container block entity")
    }
}

impl Container for BlockEntityContainer {
    fn as_any(&self) -> &dyn Any {
        self.inner().as_any()
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self.inner_mut().as_any_mut()
    }
    fn get_container_size(&self) -> usize {
        self.inner().get_container_size()
    }
    fn is_empty(&self) -> bool {
        self.inner().is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        self.inner().get_item(slot)
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        self.inner_mut().get_item_mut(slot)
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        self.inner_mut().remove_item(slot, count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.inner_mut().remove_item_no_update(slot)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        self.inner_mut().set_item(slot, stack);
    }
    fn clear_content(&mut self) {
        self.inner_mut().clear_content();
    }
    fn set_changed(&mut self) {
        self.inner_mut().set_changed();
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.inner().still_valid(player)
    }
    fn get_max_stack_size(&self) -> i32 {
        self.inner().get_max_stack_size()
    }
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        self.inner().can_place_item(slot, stack)
    }
    fn can_place_item_in_level(&self, level: &ServerLevel, slot: usize, stack: &ItemStack) -> bool {
        self.inner().can_place_item_in_level(level, slot, stack)
    }
    fn as_worldly(&self) -> Option<&dyn WorldlyContainer> {
        self.inner().as_worldly()
    }
}

/// `CompoundContainer(container1, container2)` (double chests): the second container's slots follow the first's. Not
/// worldly; `canTakeItem` keeps the interface default (`true`) exactly like vanilla.
pub struct CompoundContainer<A: Container, B: Container> {
    pub container1: A,
    pub container2: B,
}

impl<A: Container, B: Container> CompoundContainer<A, B> {
    pub fn new(container1: A, container2: B) -> Self {
        Self {
            container1,
            container2,
        }
    }
}

impl<A: Container, B: Container> Container for CompoundContainer<A, B> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.container1.get_container_size() + self.container2.get_container_size()
    }
    fn is_empty(&self) -> bool {
        self.container1.is_empty() && self.container2.is_empty()
    }
    fn get_item(&self, slot: usize) -> &ItemStack {
        let n = self.container1.get_container_size();
        if slot >= n {
            self.container2.get_item(slot - n)
        } else {
            self.container1.get_item(slot)
        }
    }
    fn get_item_mut(&mut self, slot: usize) -> Option<&mut ItemStack> {
        let n = self.container1.get_container_size();
        if slot >= n {
            self.container2.get_item_mut(slot - n)
        } else {
            self.container1.get_item_mut(slot)
        }
    }
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        let n = self.container1.get_container_size();
        if slot >= n {
            self.container2.remove_item(slot - n, count)
        } else {
            self.container1.remove_item(slot, count)
        }
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        let n = self.container1.get_container_size();
        if slot >= n {
            self.container2.remove_item_no_update(slot - n)
        } else {
            self.container1.remove_item_no_update(slot)
        }
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        let n = self.container1.get_container_size();
        if slot >= n {
            self.container2.set_item(slot - n, stack)
        } else {
            self.container1.set_item(slot, stack)
        }
    }
    fn clear_content(&mut self) {
        self.container1.clear_content();
        self.container2.clear_content();
    }
    fn set_changed(&mut self) {
        self.container1.set_changed();
        self.container2.set_changed();
    }
    fn still_valid(&self, player: &dyn MenuPlayer) -> bool {
        self.container1.still_valid(player) && self.container2.still_valid(player)
    }
    fn get_max_stack_size(&self) -> i32 {
        self.container1.get_max_stack_size()
    }
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        let n = self.container1.get_container_size();
        if slot >= n {
            self.container2.can_place_item(slot - n, stack)
        } else {
            self.container1.can_place_item(slot, stack)
        }
    }
}

/// Runs `f` with the container of the block entity at `first` or, with `second`, the `CompoundContainer(first, second)` of
/// a double chest; the owners are out of the level meanwhile. Their loot tables unpack first (vanilla unpacks inside every
/// accessor) and the `setChanged()` calls of the operation update the comparators beside them afterwards. `None` when an
/// owner is gone or is no container.
pub fn with_block_entity_containers<R>(
    level: &mut ServerLevel,
    first: mc_core::pos::BlockPos,
    second: Option<mc_core::pos::BlockPos>,
    f: impl FnOnce(&mut ServerLevel, &mut dyn Container) -> R,
) -> Option<R> {
    let Some(second) = second else {
        return level.with_block_entity(first, |level, be| {
            be.unpack_loot_table(level, None);
            let result = f(level, be.as_container_mut()?);
            if be.container_pending() {
                be.container_accessed(level);
            }
            Some(result)
        })?;
    };
    let mut a = level.take_block_entity(first)?;
    let Some(mut b) = level.take_block_entity(second) else {
        level.put_back_block_entity(first, a);
        return None;
    };
    a.unpack_loot_table(level, None);
    b.unpack_loot_table(level, None);
    if a.as_container().is_none() || b.as_container().is_none() {
        level.put_back_block_entity(first, a);
        level.put_back_block_entity(second, b);
        return None;
    }
    let mut compound = CompoundContainer::new(BlockEntityContainer(a), BlockEntityContainer(b));
    let result = f(level, &mut compound);
    level.put_back_block_entity(first, compound.container1.0);
    level.put_back_block_entity(second, compound.container2.0);
    for pos in [first, second] {
        level.with_block_entity(pos, |level, be| {
            if be.container_pending() {
                be.container_accessed(level);
            }
        });
    }
    Some(result)
}

/// `AbstractContainerMenu.getRedstoneSignalFromContainer(container)`.
pub fn get_redstone_signal_from_container(container: Option<&dyn Container>) -> i32 {
    let Some(container) = container else { return 0 };
    let mut total_percent = 0.0f32;
    for i in 0..container.get_container_size() {
        let stack = container.get_item(i);
        if !stack.is_empty() {
            total_percent += stack.count() as f32 / container.get_max_stack_size_for(stack) as f32;
        }
    }
    total_percent /= container.get_container_size() as f32;
    mc_core::math::mth::lerp_discrete(total_percent, 0, 15)
}

/// `AbstractContainerMenu.getRedstoneSignalFromBlockEntity(blockEntity)`.
pub fn get_redstone_signal_from_block_entity(
    block_entity: Option<&dyn crate::world::block_entity::BlockEntity>,
) -> i32 {
    get_redstone_signal_from_container(block_entity.and_then(|be| be.as_container()))
}

/// The lazy `RandomizableContainerBlockEntity.unpackLootTable(null)` that vanilla runs inside `getItem` when
/// `getRedstoneSignalFromContainer` reads the container of the block at `pos` (a chest reads both halves of a double
/// chest, `first` before `second`, and only when `ChestBlock.getContainer` finds one). The analog output getters only
/// get a shared level, so the comparator calls this first; the block entity changes of the fill are replayed here
/// instead of inside each `setItem`.
pub fn unpack_loot_for_signal(
    level: &mut ServerLevel,
    state: mc_data::block::BlockState,
    pos: mc_core::pos::BlockPos,
) {
    use crate::world::level::block::helpers::is_instance_of;
    use crate::world::level::block::impls::chest_block::{combine, combined_positions};
    use crate::world::level::level_access::BlockGetter;
    let (first, second) = if is_instance_of(state, "ChestBlock") {
        match combined_positions(combine(level, state, pos, false)) {
            Some(positions) => positions,
            None => return,
        }
    } else {
        (pos, None)
    };
    for pos in [Some(first), second].into_iter().flatten() {
        if level
            .get_block_entity(pos)
            .is_some_and(|be| be.has_loot_table())
        {
            level.with_block_entity(pos, |level, be| {
                be.unpack_loot_table(level, None);
                if be.container_pending() {
                    be.container_accessed(level);
                }
            });
        }
    }
}

/// `ContainerHelper`.
pub mod helper {
    use super::*;

    /// `removeItem(list, slot, count)`.
    pub fn remove_item(stacks: &mut [ItemStack], slot: usize, count: i32) -> ItemStack {
        match stacks.get_mut(slot) {
            Some(s) if !s.is_empty() && count > 0 => s.split(count),
            _ => ItemStack::empty(),
        }
    }

    /// `takeItem(list, slot)`.
    pub fn take_item(stacks: &mut [ItemStack], slot: usize) -> ItemStack {
        stacks
            .get_mut(slot)
            .map_or(ItemStack::empty(), std::mem::take)
    }

    /// `clearOrCountMatchingItems(stack, predicate, amountToRemove, countingOnly)`.
    pub fn clear_or_count_matching_items_stack(
        stack: &mut ItemStack,
        predicate: &dyn Fn(&ItemStack) -> bool,
        amount_to_remove: i32,
        counting_only: bool,
    ) -> i32 {
        if stack.is_empty() || !predicate(stack) {
            0
        } else if counting_only {
            stack.count()
        } else {
            let removed = if amount_to_remove < 0 {
                stack.count()
            } else {
                amount_to_remove.min(stack.count())
            };
            stack.shrink(removed);
            removed
        }
    }

    /// `clearOrCountMatchingItems(container, predicate, amountToRemove, countingOnly)`.
    pub fn clear_or_count_matching_items(
        container: &mut dyn Container,
        predicate: &dyn Fn(&ItemStack) -> bool,
        amount_to_remove: i32,
        counting_only: bool,
    ) -> i32 {
        let mut count = 0;
        for i in 0..container.get_container_size() {
            let Some(stack) = container.get_item_mut(i) else {
                continue;
            };
            let removed = clear_or_count_matching_items_stack(
                stack,
                predicate,
                amount_to_remove - count,
                counting_only,
            );
            if removed > 0 && !counting_only && stack.is_empty() {
                container.set_item(i, ItemStack::empty());
            }
            count += removed;
        }
        count
    }

    /// `saveAllItems(output, items, alsoWhenEmpty)`: the `Items` list (`None` = nothing to write).
    pub fn save_all_items(stacks: &[ItemStack], also_when_empty: bool) -> Option<ListTag> {
        let list = save_slots(stacks.iter().enumerate());
        if list.is_empty() && !also_when_empty {
            None
        } else {
            Some(list)
        }
    }

    /// `loadAllItems(input, items)`.
    pub fn load_all_items(tag: &CompoundTag, stacks: &mut [ItemStack]) {
        for (slot, stack) in load_slots(tag.get_list_or_empty("Items")) {
            if slot < stacks.len() {
                stacks[slot] = stack;
            }
        }
    }
}

/// `ItemStackWithSlot.CODEC` list encode: `{Slot: <unsigned byte>, id, count, components}` per non-empty stack.
pub fn save_slots<'a>(stacks: impl Iterator<Item = (usize, &'a ItemStack)>) -> ListTag {
    let mut list = ListTag::new();
    for (slot, stack) in stacks {
        if !stack.is_empty() {
            let mut tag = CompoundTag::new();
            tag.put_byte("Slot", slot as u8 as i8);
            stack.save_into(&mut tag);
            list.push(Tag::Compound(tag));
        }
    }
    list
}

/// `ItemStackWithSlot.CODEC` list decode: entries that fail to decode are dropped, a missing `Slot` is 0
/// (`optionalAlwaysPresentFieldOf(UNSIGNED_BYTE, "Slot", 0)`). The slot is NOT bounds-checked here (`isValidInContainer`).
pub fn load_slots(list: &ListTag) -> Vec<(usize, ItemStack)> {
    list.compounds()
        .filter_map(|tag| {
            let slot = tag.get_byte_or("Slot", 0) as u8 as usize;
            load_item_stack(tag).map(|stack| (slot, stack))
        })
        .collect()
}

/// `SimpleContainer`.
#[derive(Clone, Debug)]
pub struct SimpleContainer {
    items: Vec<ItemStack>,
}

impl SimpleContainer {
    pub fn new(size: usize) -> Self {
        Self {
            items: vec![ItemStack::empty(); size],
        }
    }

    pub fn from_stacks(stacks: Vec<ItemStack>) -> Self {
        Self { items: stacks }
    }

    pub fn get_items(&self) -> &[ItemStack] {
        &self.items
    }

    /// `removeAllItems()`.
    pub fn remove_all_items(&mut self) -> Vec<ItemStack> {
        let removed = self
            .items
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        self.clear_content();
        removed
    }

    /// `removeItemType(itemType, count)`.
    pub fn remove_item_type(&mut self, item: Item, count: i32) -> ItemStack {
        let mut removed = ItemStack::with_count(item, 0);
        for slot in (0..self.items.len()).rev() {
            let current = &mut self.items[slot];
            if current.item() == item {
                let still_needed = count - removed.count();
                let from_slot = current.split(still_needed);
                removed.grow(from_slot.count());
                if removed.count() == count {
                    break;
                }
            }
        }
        if !removed.is_empty() {
            self.set_changed();
        }
        removed
    }

    /// `addItem(stack)`: returns the part that did not fit.
    pub fn add_item(&mut self, stack: &ItemStack) -> ItemStack {
        if stack.is_empty() {
            return ItemStack::empty();
        }
        let mut remaining = stack.copy();
        self.move_item_to_occupied_slots_with_same_type(&mut remaining);
        if remaining.is_empty() {
            return ItemStack::empty();
        }
        self.move_item_to_empty_slots(&mut remaining);
        if remaining.is_empty() {
            ItemStack::empty()
        } else {
            remaining
        }
    }

    /// `canAddItem(stack)`.
    pub fn can_add_item(&self, stack: &ItemStack) -> bool {
        self.items.iter().any(|t| {
            t.is_empty()
                || (ItemStack::is_same_item_same_components(t, stack)
                    && t.count() < t.get_max_stack_size())
        })
    }

    fn move_item_to_empty_slots(&mut self, source: &mut ItemStack) {
        for slot in 0..self.items.len() {
            if self.get_item(slot).is_empty() {
                let moved = source.copy_and_clear();
                self.set_item(slot, moved);
                return;
            }
        }
    }

    fn move_item_to_occupied_slots_with_same_type(&mut self, source: &mut ItemStack) {
        for slot in 0..self.items.len() {
            if ItemStack::is_same_item_same_components(&self.items[slot], source) {
                self.move_items_between_stacks(source, slot);
                if source.is_empty() {
                    return;
                }
            }
        }
    }

    fn move_items_between_stacks(&mut self, source: &mut ItemStack, target_slot: usize) {
        let max = self.get_max_stack_size_for(&self.items[target_slot]);
        let diff = source.count().min(max - self.items[target_slot].count());
        if diff > 0 {
            self.items[target_slot].grow(diff);
            source.shrink(diff);
            self.set_changed();
        }
    }

    /// `fromItemList(items)`: re-adds every stack with `addItem` after clearing.
    pub fn from_item_list(&mut self, items: &[ItemStack]) {
        self.clear_content();
        for stack in items {
            self.add_item(stack);
        }
    }

    /// `storeAsItemList`: the non-empty stacks in slot order.
    pub fn store_as_item_list(&self) -> Vec<ItemStack> {
        self.items
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect()
    }
}

impl Container for SimpleContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn get_container_size(&self) -> usize {
        self.items.len()
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
        // Java's `items.get(slot)` throws for an out-of-range slot; every caller passes a valid index.
        self.items
            .get_mut(slot)
            .map_or(ItemStack::empty(), std::mem::take)
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
        // `NonNullList.clear()` with a default value resets every entry to EMPTY (size is kept).
        self.items.fill(ItemStack::empty());
        self.set_changed();
    }
    fn set_changed(&mut self) {}
    fn still_valid(&self, _player: &dyn MenuPlayer) -> bool {
        true
    }
}
