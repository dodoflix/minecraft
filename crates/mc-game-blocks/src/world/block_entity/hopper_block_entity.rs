//! `Hopper` + `HopperBlockEntity`: item transfer between containers (`tryMoveItems`, `suckInItems`, `ejectItems`,
//! `addItem`), container resolution (`getContainerAt`: blocks, double chests, worldly holders, container entities) and
//! item entity absorption.
//!
//! Ownership model: containers live inside block entities / entities that sit in the level's storage. A resolved
//! container is therefore a [`ContainerTarget`] handle; [`with_container`] takes the owner out of the level (the same
//! take-out pattern as ticking) and runs a closure with `&mut dyn Container` next to `&mut ServerLevel`. The hopper that
//! ticks is itself out of its chunk while it runs, exactly like every ticking block entity.
//!
//! `Container::set_changed` of block entity containers is recorded in [`changed_log`] and replayed right after each
//! public operation (the comparator updates of vanilla's `setChanged`, in their original order).

use mc_core::direction::Direction;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_data::props;
use mc_game_core::block_entity_base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::ComponentReader;
use mc_game_core::world::block_entity::base_container::BaseContainer;
use mc_game_core::world::block_entity::randomizable_container::{
    RandomizableContainerData, unpack_loot_table,
};
use mc_game_core::world::block_entity::{
    BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration, changed_log,
};
use mc_game_core::world::container::{
    Container, empty_stack, helper, with_block_entity_containers,
};
use mc_game_core::world::entity::item::ItemEntity;
use mc_game_core::world::entity::{Entity, EntityId};
use mc_game_core::world::inventory::MenuPlayer;
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::block::{BlockStateExt, behaviour};
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use mc_nbt::CompoundTag;
use std::any::Any;

pub const MOVE_ITEM_SPEED: i32 = 8;
pub const HOPPER_CONTAINER_SIZE: usize = 5;
const NO_COOLDOWN_TIME: i32 = -1;

/// `Hopper.SUCK_AABB` = `Block.column(16, 11, 32).toAabbs().get(0)`.
pub fn suck_aabb() -> AABB {
    AABB::new(0.0, 11.0 / 16.0, 0.0, 1.0, 32.0 / 16.0, 1.0)
}

/// `Hopper` (also implemented by the hopper minecart).
pub trait Hopper: Container {
    fn get_suck_aabb(&self) -> AABB {
        suck_aabb()
    }
    fn get_level_x(&self) -> f64;
    fn get_level_y(&self) -> f64;
    fn get_level_z(&self) -> f64;
    fn is_grid_aligned(&self) -> bool;
    /// The lazy loot table unpack of `getItem` for a hopper whose container is an entity (a block entity hopper unpacks in
    /// its own tick): called before items are moved into the hopper.
    fn unpack_pending_loot(&mut self, _level: &ServerLevel) {}
}

pub struct HopperBlockEntity {
    base: BlockEntityData,
    items: Vec<ItemStack>,
    cooldown_time: i32,
    ticked_game_time: i64,
    facing: Direction,
    pub loot: RandomizableContainerData,
    pub container: BaseContainer,
}

impl HopperBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self {
            base: BlockEntityData::new(BlockEntityType::HOPPER, pos, state),
            items: vec![ItemStack::empty(); HOPPER_CONTAINER_SIZE],
            cooldown_time: NO_COOLDOWN_TIME,
            ticked_game_time: 0,
            facing: state.get(props::FACING_HOPPER),
            loot: Default::default(),
            container: Default::default(),
        }
    }

    pub fn default_name_key(&self) -> &'static str {
        "container.hopper"
    }

    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }

    /// `setCooldown(time)`.
    fn set_cooldown(&mut self, time: i32) {
        self.cooldown_time = time;
    }

    fn is_on_cooldown(&self) -> bool {
        self.cooldown_time > 0
    }

    /// `isOnCustomCooldown()`.
    fn is_on_custom_cooldown(&self) -> bool {
        self.cooldown_time > 8
    }

    /// `inventoryFull()`.
    fn inventory_full(&self) -> bool {
        self.items
            .iter()
            .all(|s| !s.is_empty() && s.count() == s.get_max_stack_size())
    }
}

impl BlockEntity for HopperBlockEntity {
    block_entity_base!();

    fn load_additional(&mut self, tag: &CompoundTag) {
        self.container.load(tag);
        self.items = vec![ItemStack::empty(); HOPPER_CONTAINER_SIZE];
        if !self.loot.try_load_loot_table(tag) {
            helper::load_all_items(tag, &mut self.items);
        }
        self.cooldown_time = tag.get_int_or("TransferCooldown", -1);
    }

    fn save_additional(&self, tag: &mut CompoundTag) {
        self.container.save(tag);
        if !self.loot.try_save_loot_table(tag)
            && let Some(list) = helper::save_all_items(&self.items, true)
        {
            tag.put("Items", list);
        }
        tag.put_int("TransferCooldown", self.cooldown_time);
    }

    fn set_block_state(&mut self, state: BlockState) {
        self.base.block_state = state;
        self.facing = state.get(props::FACING_HOPPER);
    }

    fn is_ticking(&self) -> bool {
        true
    }

    fn server_tick(&mut self, level: &mut ServerLevel) {
        let (pos, state) = (self.base.pos, self.base.block_state);
        push_items_tick(level, pos, state, self);
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

    fn has_loot_table(&self) -> bool {
        self.loot.loot_table.is_some()
    }

    fn set_loot_table(&mut self, table: mc_core::ident::Identifier, seed: i64) {
        self.loot.set_loot_table(table, seed);
    }

    fn collect_implicit_components(
        &self,
        builder: &mut mc_protocol::components::map::DataComponentMapBuilder,
    ) {
        *builder = self
            .container
            .collect_implicit_components(std::mem::take(builder), &self.items);
        *builder = self
            .loot
            .collect_implicit_components(std::mem::take(builder));
    }

    /// `removeComponentsFromTag(output)`.
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        self.container.remove_components_from_tag(tag);
        self.loot.remove_components_from_tag(tag);
    }

    fn apply_implicit_components(&mut self, components: &ComponentReader) {
        self.container
            .apply_implicit_components(components, &mut self.items);
        self.loot.apply_implicit_components(components);
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

impl Container for HopperBlockEntity {
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
    /// `HopperBlockEntity.removeItem`: unlike `BaseContainerBlockEntity` it does NOT call `setChanged`.
    fn remove_item(&mut self, slot: usize, count: i32) -> ItemStack {
        helper::remove_item(&mut self.items, slot, count)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        helper::take_item(&mut self.items, slot)
    }
    /// `HopperBlockEntity.setItem`: no `setChanged` either.
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let max = self.get_max_stack_size_for(&stack);
        stack.limit_size(max);
        if let Some(s) = self.items.get_mut(slot) {
            *s = stack;
        }
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
}

impl Hopper for HopperBlockEntity {
    fn get_level_x(&self) -> f64 {
        f64::from(self.base.pos.x()) + 0.5
    }
    fn get_level_y(&self) -> f64 {
        f64::from(self.base.pos.y()) + 0.5
    }
    fn get_level_z(&self) -> f64 {
        f64::from(self.base.pos.z()) + 0.5
    }
    fn is_grid_aligned(&self) -> bool {
        true
    }
}

// ------------------------------------------------------------------------------------------------ ticking / moving

/// `HopperBlockEntity.pushItemsTick(level, pos, state, entity)`.
pub fn push_items_tick(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    entity: &mut HopperBlockEntity,
) {
    entity.cooldown_time = entity.cooldown_time.wrapping_sub(1);
    entity.ticked_game_time = level.game_time();
    if !entity.is_on_cooldown() {
        entity.set_cooldown(0);
        try_move_items(level, pos, state, entity, |level, entity| {
            suck_in_items(level, entity)
        });
    }
}

/// `tryMoveItems(level, pos, state, entity, action)`.
fn try_move_items(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    entity: &mut HopperBlockEntity,
    action: impl FnOnce(&mut ServerLevel, &mut HopperBlockEntity) -> bool,
) -> bool {
    if !entity.is_on_cooldown() && state.get(props::ENABLED) {
        let mut changed = false;
        // `isEmpty()` unpacks the loot table first
        if entity.has_loot_table() {
            entity.unpack_loot_table(level, None);
            changed_log::flush(level);
        }
        if !entity.is_empty() {
            changed = eject_items(level, pos, entity);
        }
        if !entity.inventory_full() {
            changed |= action(level, entity);
        }
        if changed {
            entity.set_cooldown(MOVE_ITEM_SPEED);
            // the static `setChanged(level, pos, state)` uses the state the caller passed
            mc_game_core::world::block_entity::set_changed(level, pos, state);
            return true;
        }
    }
    false
}

/// `ejectItems(level, pos, self)`.
fn eject_items(level: &mut ServerLevel, pos: BlockPos, this: &mut HopperBlockEntity) -> bool {
    let Some(target) = get_container_at(level, pos.relative(this.facing)) else {
        return false;
    };
    let direction = this.facing.get_opposite();
    let moved = with_container(level, &target, |level, container| {
        if is_full_container(container, direction) {
            return false;
        }
        for slot in 0..this.get_container_size() {
            if !this.get_item(slot).is_empty() {
                let original_count = this.get_item(slot).count();
                let one = this.remove_item(slot, 1);
                let result = add_item(level, Some(&*this), container, one, Some(direction));
                if result.is_empty() {
                    container.set_changed();
                    return true;
                }
                this.items[slot].set_count(original_count);
                if original_count == 1 {
                    let stack = this.items[slot].clone();
                    this.set_item(slot, stack);
                }
            }
        }
        false
    })
    .unwrap_or(false);
    changed_log::flush(level);
    moved
}

/// The slot indices of `getSlots(container, direction)`.
enum Slots {
    Listed(&'static [usize]),
    Flat(usize),
}

impl Slots {
    fn len(&self) -> usize {
        match self {
            Slots::Listed(s) => s.len(),
            Slots::Flat(n) => *n,
        }
    }
    fn at(&self, i: usize) -> usize {
        match self {
            Slots::Listed(s) => s[i],
            Slots::Flat(_) => i,
        }
    }
}

/// `getSlots(container, direction)`.
fn get_slots(container: &dyn Container, direction: Direction) -> Slots {
    match container.as_worldly() {
        Some(worldly) => Slots::Listed(worldly.get_slots_for_face(direction)),
        None => Slots::Flat(container.get_container_size()),
    }
}

/// `isFullContainer(container, direction)`.
fn is_full_container(container: &dyn Container, direction: Direction) -> bool {
    let slots = get_slots(container, direction);
    (0..slots.len()).all(|i| {
        let stack = container.get_item(slots.at(i));
        stack.count() >= stack.get_max_stack_size()
    })
}

/// `suckInItems(level, hopper)`.
pub fn suck_in_items(level: &mut ServerLevel, hopper: &mut dyn Hopper) -> bool {
    let block_pos = BlockPos::containing(
        hopper.get_level_x(),
        hopper.get_level_y() + 1.0,
        hopper.get_level_z(),
    );
    let state = BlockGetter::get_block_state(level, block_pos);
    let source = get_container_at_xyz(
        level,
        block_pos,
        state,
        hopper.get_level_x(),
        hopper.get_level_y() + 1.0,
        hopper.get_level_z(),
    );
    let result = if let Some(target) = source {
        with_container(level, &target, |level, container| {
            let direction = Direction::Down;
            let slots = get_slots(container, direction);
            for i in 0..slots.len() {
                if try_take_in_item_from_slot(level, hopper, container, slots.at(i), direction) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
    } else {
        let is_blocked = hopper.is_grid_aligned()
            && state.is_collision_shape_full_block_at(level, block_pos)
            && !state.is_tag("does_not_block_hoppers");
        let mut moved = false;
        if !is_blocked {
            for id in get_items_at_and_above(level, &*hopper) {
                if absorb_item_entity(level, hopper, id) {
                    moved = true;
                    break;
                }
            }
        }
        moved
    };
    changed_log::flush(level);
    result
}

/// `tryTakeInItemFromSlot(hopper, container, slot, direction)`.
fn try_take_in_item_from_slot(
    level: &ServerLevel,
    hopper: &mut dyn Hopper,
    container: &mut dyn Container,
    slot: usize,
    direction: Direction,
) -> bool {
    let stack = container.get_item(slot);
    if !stack.is_empty() && can_take_item_from_container(hopper, container, stack, slot, direction)
    {
        let original_count = stack.count();
        let one = container.remove_item(slot, 1);
        hopper.unpack_pending_loot(level);
        let result = add_item(level, Some(&*container), hopper, one, None);
        if result.is_empty() {
            container.set_changed();
            return true;
        }
        if let Some(stack) = container.get_item_mut(slot) {
            stack.set_count(original_count);
        }
        if original_count == 1 {
            let stack = container.get_item(slot).clone();
            container.set_item(slot, stack);
        }
    }
    false
}

/// `addItem(hopper, itemEntity)` for the item entity `id` (false when it is gone or not an item).
pub fn absorb_item_entity(level: &mut ServerLevel, hopper: &mut dyn Hopper, id: EntityId) -> bool {
    level
        .with_entity(id, |level, entity| {
            let Some(item) = entity.as_any_mut().downcast_mut::<ItemEntity>() else {
                return false;
            };
            hopper.unpack_pending_loot(level);
            add_item_entity(level, hopper, item)
        })
        .unwrap_or(false)
}

/// `addItem(container, itemEntity)`: absorbs what fits, discards the entity when everything moved.
pub fn add_item_entity(
    level: &ServerLevel,
    container: &mut dyn Container,
    entity: &mut ItemEntity,
) -> bool {
    let copy = entity.get_item().copy();
    let result = add_item(level, None, container, copy, None);
    if result.is_empty() {
        entity.set_item(ItemStack::empty());
        mc_game_core::world::entity::Entity::discard(entity);
        true
    } else {
        entity.set_item(result);
        false
    }
}

/// `addItem(from, container, itemStack, direction)`: the remainder that did not fit.
pub fn add_item(
    level: &ServerLevel,
    from: Option<&dyn Container>,
    container: &mut dyn Container,
    mut stack: ItemStack,
    direction: Option<Direction>,
) -> ItemStack {
    let worldly_slots = match (container.as_worldly(), direction) {
        (Some(worldly), Some(direction)) => Some(worldly.get_slots_for_face(direction)),
        _ => None,
    };
    if let Some(slots) = worldly_slots {
        let mut i = 0;
        while i < slots.len() && !stack.is_empty() {
            stack = try_move_in_item(level, from, container, stack, slots[i], direction);
            i += 1;
        }
    } else {
        let size = container.get_container_size();
        let mut i = 0;
        while i < size && !stack.is_empty() {
            stack = try_move_in_item(level, from, container, stack, i, direction);
            i += 1;
        }
    }
    stack
}

/// `canPlaceItemInContainer(container, itemStack, slot, direction)`.
fn can_place_item_in_container(
    level: &ServerLevel,
    container: &dyn Container,
    stack: &ItemStack,
    slot: usize,
    direction: Option<Direction>,
) -> bool {
    if !container.can_place_item_in_level(level, slot, stack) {
        return false;
    }
    match container.as_worldly() {
        Some(worldly) => worldly.can_place_item_through_face(level, slot, stack, direction),
        None => true,
    }
}

/// `canTakeItemFromContainer(into, from, itemStack, slot, direction)`.
fn can_take_item_from_container(
    into: &dyn Container,
    from: &dyn Container,
    stack: &ItemStack,
    slot: usize,
    direction: Direction,
) -> bool {
    if !from.can_take_item(into, slot, stack) {
        return false;
    }
    match from.as_worldly() {
        Some(worldly) => worldly.can_take_item_through_face(slot, stack, direction),
        None => true,
    }
}

/// `canMergeItems(a, b)`.
fn can_merge_items(a: &ItemStack, b: &ItemStack) -> bool {
    a.count() <= a.get_max_stack_size() && ItemStack::is_same_item_same_components(a, b)
}

/// `tryMoveInItem(from, container, itemStack, slot, direction)`.
fn try_move_in_item(
    level: &ServerLevel,
    from: Option<&dyn Container>,
    container: &mut dyn Container,
    mut stack: ItemStack,
    slot: usize,
    direction: Option<Direction>,
) -> ItemStack {
    if can_place_item_in_container(level, container, &stack, slot, direction) {
        let mut success = false;
        let was_empty = container.is_empty();
        if container.get_item(slot).is_empty() {
            container.set_item(slot, stack);
            stack = ItemStack::empty();
            success = true;
        } else if can_merge_items(container.get_item(slot), &stack) {
            let current = container.get_item(slot);
            let space = stack.get_max_stack_size() - current.count();
            let count = stack.count().min(space);
            stack.shrink(count);
            if let Some(current) = container.get_item_mut(slot) {
                current.grow(count);
            }
            success = count > 0;
        }
        if success {
            let from_ticked = from
                .and_then(|f| f.as_any().downcast_ref::<HopperBlockEntity>())
                .map(|h| h.ticked_game_time);
            if was_empty
                && let Some(hopper) = container.as_any_mut().downcast_mut::<HopperBlockEntity>()
                && !hopper.is_on_custom_cooldown()
            {
                let mut skip_tick_count = 0;
                if let Some(from_ticked) = from_ticked
                    && hopper.ticked_game_time >= from_ticked
                {
                    skip_tick_count = 1;
                }
                hopper.set_cooldown(MOVE_ITEM_SPEED - skip_tick_count);
            }
            container.set_changed();
        }
    }
    stack
}

// -------------------------------------------------------------------------------------------- container resolution

/// A resolved `Container` (the result of `getContainerAt`), usable through [`with_container`].
#[derive(Clone, Debug)]
pub enum ContainerTarget {
    /// A block entity that is a `Container`.
    BlockEntity(BlockPos),
    /// `ChestBlock.getContainer(.., true)` of a double chest: `CompoundContainer(first, second)`.
    DoubleChest(BlockPos, BlockPos),
    /// `WorldlyContainerHolder.getContainer(state, level, pos)` (composter).
    Holder(BlockPos, BlockState),
    /// A container entity (minecart with chest/hopper, chest boat, ...).
    Entity(EntityId),
}

/// `getContainerAt(level, pos)`.
pub fn get_container_at(level: &mut ServerLevel, pos: BlockPos) -> Option<ContainerTarget> {
    let state = BlockGetter::get_block_state(level, pos);
    get_container_at_xyz(
        level,
        pos,
        state,
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    )
}

fn get_container_at_xyz(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    x: f64,
    y: f64,
    z: f64,
) -> Option<ContainerTarget> {
    get_block_container(level, pos, state).or_else(|| get_entity_container(level, x, y, z))
}

/// `getBlockContainer(level, pos, state)`.
fn get_block_container(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
) -> Option<ContainerTarget> {
    let behaviour = behaviour(state.block());
    if behaviour.is_worldly_container_holder() {
        return Some(ContainerTarget::Holder(pos, state));
    }
    if !state.has_block_entity()
        || !level
            .get_block_entity_immediate(pos)
            .is_some_and(|be| be.as_container().is_some())
    {
        return None;
    }
    if state.block().data().classes.contains(&"ChestBlock") {
        return Some(combine_chest(level, pos, state));
    }
    Some(ContainerTarget::BlockEntity(pos))
}

/// `ChestBlock.getContainer(block, state, level, pos, true)` for a chest whose block entity is a container.
fn combine_chest(level: &mut ServerLevel, pos: BlockPos, state: BlockState) -> ContainerTarget {
    use mc_data::enums::ChestType;
    use mc_game_core::world::level::block::impls::chest_block::get_connected_direction;
    let ty = state.get(props::CHEST_TYPE);
    if ty == ChestType::Single {
        return ContainerTarget::BlockEntity(pos);
    }
    let neighbour_pos = pos.relative(get_connected_direction(state));
    let neighbour_state = BlockGetter::get_block_state(level, neighbour_pos);
    if neighbour_state.is_block(state.block()) {
        let neighbour_type = neighbour_state.get(props::CHEST_TYPE);
        if neighbour_type != ChestType::Single
            && ty != neighbour_type
            && neighbour_state.get(props::HORIZONTAL_FACING) == state.get(props::HORIZONTAL_FACING)
            && level
                .get_block_entity_immediate(neighbour_pos)
                .is_some_and(|be| be.as_container().is_some())
        {
            // `ChestType.RIGHT` is `DoubleBlockCombiner.BlockType.FIRST`
            return if ty == ChestType::Right {
                ContainerTarget::DoubleChest(pos, neighbour_pos)
            } else {
                ContainerTarget::DoubleChest(neighbour_pos, pos)
            };
        }
    }
    ContainerTarget::BlockEntity(pos)
}

/// `EntitySelector.CONTAINER_ENTITY_SELECTOR` type test (`entity instanceof Container`): the chest/hopper minecarts and the
/// chest boats, every `ContainerEntity` (horse inventories and the like are not `Container` entities).
fn is_container_entity(entity: &dyn Entity) -> bool {
    entity.chest_vehicle().is_some()
}

/// `getEntityContainer(level, x, y, z)`: one of the container entities in the 1x1x1 box, picked with the level random.
fn get_entity_container(
    level: &mut ServerLevel,
    x: f64,
    y: f64,
    z: f64,
) -> Option<ContainerTarget> {
    let aabb = AABB::new(x - 0.5, y - 0.5, z - 0.5, x + 0.5, y + 0.5, z + 0.5);
    let entities = level.get_entities(None, &aabb, |e| is_container_entity(e) && e.is_alive());
    if entities.is_empty() {
        return None;
    }
    let index = level.random().next_int_bounded(entities.len() as i32) as usize;
    Some(ContainerTarget::Entity(entities[index]))
}

/// Runs `f` with the container of `target` (its owner is out of the level meanwhile). Block entity and entity containers
/// unpack their loot table first (vanilla unpacks inside every accessor). `None` when the owner disappeared.
pub fn with_container<R>(
    level: &mut ServerLevel,
    target: &ContainerTarget,
    f: impl FnOnce(&mut ServerLevel, &mut dyn Container) -> R,
) -> Option<R> {
    match target {
        ContainerTarget::BlockEntity(pos) => with_block_entity_containers(level, *pos, None, f),
        ContainerTarget::DoubleChest(first, second) => {
            with_block_entity_containers(level, *first, Some(*second), f)
        }
        ContainerTarget::Holder(pos, state) => {
            let behaviour = behaviour(state.block());
            let mut container = behaviour.get_worldly_container(*state, level, *pos)?;
            let result = f(level, container.as_mut());
            behaviour.finish_worldly_container(*state, level, *pos, container);
            Some(result)
        }
        ContainerTarget::Entity(id) => {
            let mut entity = level.take_entity(*id)?;
            let position = entity.position();
            if let Some(chest) = entity.chest_vehicle_mut() {
                chest.unpack_for_access(level, position);
            }
            let result = entity
                .as_container_mut()
                .map(|container| f(level, container));
            level.put_back_entity(*id, entity);
            result
        }
    }
}

/// `getItemsAtAndAbove(level, hopper)`: the living item entities in the suck box.
pub fn get_items_at_and_above(level: &ServerLevel, hopper: &dyn Hopper) -> Vec<EntityId> {
    let aabb = hopper.get_suck_aabb().move_(
        hopper.get_level_x() - 0.5,
        hopper.get_level_y() - 0.5,
        hopper.get_level_z() - 0.5,
    );
    level.get_entities_of_class(&aabb, |e| e.as_any().is::<ItemEntity>(), |e| e.is_alive())
}

/// `HopperBlockEntity.entityInside(level, pos, state, entity, hopper)`: an item entity touching the suck box is moved in
/// right away (subject to the cooldown / enabled checks of `tryMoveItems`).
pub fn entity_inside(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    entity: &mut ItemEntity,
) {
    use mc_game_core::world::entity::Entity;
    if entity.get_item().is_empty() {
        return;
    }
    let suck = suck_aabb();
    let bb = entity.get_bounding_box().move_(
        -f64::from(pos.x()),
        -f64::from(pos.y()),
        -f64::from(pos.z()),
    );
    if !bb.intersects_aabb(&suck) {
        return;
    }
    level.with_block_entity(pos, |level, be| {
        if let Some(hopper) = be.as_any_mut().downcast_mut::<HopperBlockEntity>() {
            try_move_items(level, pos, state, hopper, |level, hopper| {
                add_item_entity(level, hopper, entity)
            });
            changed_log::flush(level);
        }
    });
}

#[linkme::distributed_slice(BLOCK_ENTITY_TYPES)]
static HOPPER: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::HOPPER,
    create: |pos, state| Box::new(HopperBlockEntity::new(pos, state)),
};
