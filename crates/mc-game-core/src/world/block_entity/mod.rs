//! `net.minecraft.world.level.block.entity`: block entities are `Box<dyn BlockEntity>` stored in the level's chunk
//! (same take-out-while-ticking pattern as entities). Concrete types register a factory through
//! [`BLOCK_ENTITY_TYPES`] from the module that implements them (one file per vanilla class); types without an
//! implementation are kept as raw NBT by mc-world so nothing is lost on save.

/// The accessors every implementation of [`BlockEntity`] needs (the struct keeps its [`BlockEntityData`] in `base`).
#[macro_export]
macro_rules! block_entity_base {
    () => {
        fn base(&self) -> &BlockEntityData {
            &self.base
        }
        fn base_mut(&mut self) -> &mut BlockEntityData {
            &mut self.base
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

/// `Container` for a block entity backed by `$items: Vec<ItemStack>` with `BaseContainerBlockEntity`'s semantics
/// (`setChanged` is deferred: it sets the `changed` flag, the level side effects run in `container_accessed`). Extra
/// trait items go in the trailing braces.
#[macro_export]
macro_rules! impl_items_container {
    ($ty:ty, $items:ident $({ $($extra:tt)* })?) => {
        impl $crate::world::container::Container for $ty {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
            fn get_container_size(&self) -> usize {
                self.$items.len()
            }
            fn is_empty(&self) -> bool {
                self.$items.iter().all(|s| s.is_empty())
            }
            fn get_item(&self, slot: usize) -> &$crate::world::item::ItemStack {
                self.$items.get(slot).unwrap_or_else(|| $crate::world::container::empty_stack())
            }
            fn get_item_mut(&mut self, slot: usize) -> Option<&mut $crate::world::item::ItemStack> {
                self.$items.get_mut(slot)
            }
            fn remove_item(&mut self, slot: usize, count: i32) -> $crate::world::item::ItemStack {
                let result = $crate::world::container::helper::remove_item(&mut self.$items, slot, count);
                if !result.is_empty() {
                    $crate::world::container::Container::set_changed(self);
                }
                result
            }
            fn remove_item_no_update(&mut self, slot: usize) -> $crate::world::item::ItemStack {
                $crate::world::container::helper::take_item(&mut self.$items, slot)
            }
            fn set_item(&mut self, slot: usize, mut stack: $crate::world::item::ItemStack) {
                if slot < self.$items.len() {
                    let max = $crate::world::container::Container::get_max_stack_size_for(self, &stack);
                    stack.limit_size(max);
                    self.$items[slot] = stack;
                }
                $crate::world::container::Container::set_changed(self);
            }
            fn clear_content(&mut self) {
                self.$items.iter_mut().for_each(|s| *s = $crate::world::item::ItemStack::empty());
            }
            fn set_changed(&mut self) {
                self.base.changed = true;
            }
            fn still_valid(&self, player: &dyn $crate::world::inventory::MenuPlayer) -> bool {
                $crate::world::container::still_valid_block_entity(self.base.pos, player)
            }
            $($($extra)*)?
        }
    };
}

pub mod barrel_block_entity;
pub mod base_container;
mod beehive_block_entity;
mod bell_block_entity;
pub mod campfire_block_entity;
pub mod changed_log;
pub mod chest_block_entity;
pub mod command_block_entity;
pub mod container_menus;
pub mod container_openers;
pub mod copper_golem_statue_block_entity;
pub mod deferred;
pub mod dispenser_block_entity;
pub mod jigsaw_block_entity;
pub mod randomizable_container;
pub mod sculk_sensor_block_entity;
mod sculk_shrieker_block_entity;
pub mod shulker_box_block_entity;
pub mod sign_block_entity;
pub mod structure_block_entity;
pub mod test_block_entity;
pub mod test_instance_block_entity;
mod the_end_gateway_block_entity;
pub mod vibration;

pub use beehive_block_entity::{BeeReleaseStatus, BeehiveBlockEntity};
pub use bell_block_entity::BellBlockEntity;
pub use command_block_entity::CommandBlockEntity;
pub use copper_golem_statue_block_entity::CopperGolemStatueBlockEntity;
pub use dispenser_block_entity::DispenserBlockEntity;
pub use jigsaw_block_entity::JigsawBlockEntity;
pub use sculk_sensor_block_entity::SculkSensorBlockEntity;
pub use sculk_shrieker_block_entity::{SculkShriekerBlockEntity, apply_darkness_around};
pub use sign_block_entity::SignBlockEntity;
pub use structure_block_entity::StructureBlockEntity;
pub use test_block_entity::TestBlockEntity;
pub use test_instance_block_entity::TestInstanceBlockEntity;
pub use the_end_gateway_block_entity::TheEndGatewayBlockEntity;

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::DataComponentType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use std::any::Any;

/// `net.minecraft.world.level.Spawner`: block entities / entities whose spawned entity a spawn egg or `/data` can set.
pub trait Spawner {
    /// `setEntityId(type, random)` (the random is the level's).
    fn set_entity_id(
        &mut self,
        level: &mut crate::server::level::ServerLevel,
        ty: mc_data::EntityType,
    );
    /// `setEntityData(data, random)`: `data.tag` is the compound without `id`.
    fn set_entity_data(
        &mut self,
        level: &mut crate::server::level::ServerLevel,
        ty: mc_data::EntityType,
        data: &CompoundTag,
    );
}

/// Fields every block entity has (vanilla `BlockEntity`).
#[derive(Clone, Debug)]
pub struct BlockEntityData {
    pub ty: BlockEntityType,
    pub pos: BlockPos,
    pub block_state: BlockState,
    pub removed: bool,
    /// `BlockEntity.components` (`DataComponentMap.EMPTY` until set).
    pub components: mc_protocol::components::DataComponentMap,
    /// `setChanged()` was requested by a container operation that had no level at hand (see
    /// `BlockEntity::flush_pending`).
    pub changed: bool,
    /// Identity of this block entity object (vanilla compares references: `level.getBlockEntity(pos) != blockEntity`
    /// in `Container.stillValidBlockEntity`).
    pub instance_id: u64,
}

static NEXT_INSTANCE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl BlockEntityData {
    pub fn new(ty: BlockEntityType, pos: BlockPos, block_state: BlockState) -> Self {
        Self {
            ty,
            pos,
            block_state,
            removed: false,
            components: Default::default(),
            changed: false,
            instance_id: NEXT_INSTANCE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// Vanilla `BlockEntity` (overridable behavior; defaults = the base class).
/// `ContainerUser` of the player that opens/closes a block-entity container (the player itself is not reachable
/// through the level while its packet is handled).
#[derive(Clone, Copy, Debug)]
pub struct ContainerUser {
    pub id: crate::world::entity::EntityId,
    pub spectator: bool,
    /// `getContainerInteractionRange()` (`BLOCK_INTERACTION_RANGE` attribute value).
    pub interaction_range: f64,
}

/// The `DataComponentGetter` `BlockEntity.applyComponents` hands to `applyImplicitComponents`: the full view of the stack
/// (prototype plus patch) that remembers which component types were asked for, present or not.
pub struct ComponentReader {
    full: mc_protocol::components::map::PatchedDataComponentMap,
    read: std::cell::RefCell<Vec<DataComponentType>>,
}

impl ComponentReader {
    fn new(
        prototype: mc_protocol::components::DataComponentMap,
        patch: &mc_protocol::components::DataComponentPatch,
    ) -> Self {
        Self {
            full: mc_protocol::components::map::PatchedDataComponentMap::from_patch(
                prototype, patch,
            ),
            read: Default::default(),
        }
    }

    fn into_read(self) -> Vec<DataComponentType> {
        self.read.into_inner()
    }

    /// `get(type)`.
    pub fn get<T: 'static>(&self, key: &mc_protocol::components::ComponentKey<T>) -> Option<&T> {
        self.read.borrow_mut().push(key.id());
        mc_protocol::components::map::DataComponentGetter::get(&self.full, key)
    }

    /// `getOrDefault(type, default)`.
    pub fn get_or_default<'a, T: 'static>(
        &'a self,
        key: &mc_protocol::components::ComponentKey<T>,
        default: &'a T,
    ) -> &'a T {
        self.get(key).unwrap_or(default)
    }
}

pub trait BlockEntity: Any {
    fn base(&self) -> &BlockEntityData;
    fn base_mut(&mut self) -> &mut BlockEntityData;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn get_type(&self) -> BlockEntityType {
        self.base().ty
    }
    fn get_block_pos(&self) -> BlockPos {
        self.base().pos
    }
    fn get_block_state(&self) -> BlockState {
        self.base().block_state
    }
    fn is_removed(&self) -> bool {
        self.base().removed
    }
    fn set_removed(&mut self) {
        self.base_mut().removed = true;
    }
    /// `loadAdditional(input)`: read the type-specific part of the saved NBT.
    fn load_additional(&mut self, _tag: &CompoundTag) {}
    /// `saveAdditional(output)`: write the type-specific part (the common `id`/`x`/`y`/`z` part is added by the caller).
    fn save_additional(&self, _tag: &mut CompoundTag) {}
    /// `saveWithoutMetadata(output)`: `saveAdditional` and the `components` map.
    fn save_without_metadata(&self, tag: &mut CompoundTag) {
        self.save_additional(tag);
        if let Ok(components) = mc_dfu::encode_nbt(
            &mc_protocol::components::DataComponentMap::codec(),
            &self.base().components,
        )
        .into_result()
        {
            tag.put("components", components);
        }
    }
    /// `saveWithFullMetadata(registries)`: `saveAdditional`, the `components` map, then `id`, `x`, `y`, `z`.
    fn save_with_full_metadata(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        self.save_without_metadata(&mut tag);
        tag.put_string("id", self.get_type().name());
        let pos = self.get_block_pos();
        tag.put_int("x", pos.x());
        tag.put_int("y", pos.y());
        tag.put_int("z", pos.z());
        tag
    }
    /// `loadWithComponents(input)`: `loadAdditional`, then `components` (empty when absent or malformed).
    fn load_with_components(&mut self, tag: &CompoundTag) {
        self.load_additional(tag);
        self.base_mut().components = read_codec(
            tag,
            "components",
            &mc_protocol::components::DataComponentMap::codec(),
        )
        .unwrap_or_default();
    }
    /// `collectImplicitComponents(builder)`: the components a block entity derives from its own state (container contents,
    /// custom name, banner patterns, ...), written on top of `components` by `collectComponents`.
    fn collect_implicit_components(
        &self,
        _builder: &mut mc_protocol::components::map::DataComponentMapBuilder,
    ) {
    }
    /// `collectComponents()`: the stored `components` plus the implicit ones.
    fn collect_components(&self) -> mc_protocol::components::DataComponentMap {
        let mut builder =
            mc_protocol::components::DataComponentMap::builder().add_all(&self.base().components);
        self.collect_implicit_components(&mut builder);
        builder.build()
    }
    /// `removeComponentsFromTag(output)`: drops from the `saveCustomOnly` NBT the keys that `collectComponents` carries as
    /// components instead (pick block with data).
    fn remove_components_from_tag(&self, _tag: &mut CompoundTag) {}
    /// `Nameable.getCustomName()` (block entities that have one).
    fn get_custom_name(&self) -> Option<mc_text::Component> {
        None
    }
    /// `this instanceof SlotProvider` (containers readable through slot ranges).
    fn as_slot_provider(&self) -> Option<&dyn crate::world::item::slot::SlotProvider> {
        None
    }
    /// `applyImplicitComponents(components)`: block entities that derive state from item components (custom name,
    /// container contents, ...) override this. Every component read through `components` is consumed: it does not
    /// stay in the stored `components` map ([`BlockEntity::apply_components`]).
    fn apply_implicit_components(&mut self, _components: &ComponentReader) {}
    /// `applyComponents(prototype, patch)` (do not override): `applyImplicitComponents`, then every patch entry that was
    /// not read there (and is not `block_entity_data`/`block_state`) is kept in the stored `components`; removals are dropped.
    fn apply_components(
        &mut self,
        prototype: mc_protocol::components::DataComponentMap,
        patch: &mc_protocol::components::DataComponentPatch,
    ) {
        let reader = ComponentReader::new(prototype, patch);
        self.apply_implicit_components(&reader);
        let implicit = reader.into_read();
        let kept = patch.forget(|ty| {
            ty == DataComponentType::BLOCK_ENTITY_DATA
                || ty == DataComponentType::BLOCK_STATE
                || implicit.contains(&ty)
        });
        self.base_mut().components = kept.split().0;
    }
    /// `applyComponentsFromItemStack(stack)` (do not override).
    fn apply_components_from_item_stack(&mut self, stack: &crate::world::item::ItemStack) {
        self.apply_components(stack.get_prototype(), &stack.get_components_patch());
    }
    /// `getUpdateTag`: the NBT sent to clients in chunk packets / block entity data packets (default empty).
    fn get_update_tag(&self) -> CompoundTag {
        CompoundTag::new()
    }
    /// `getUpdatePacket()`: the `ClientboundBlockEntityDataPacket` for clients (`null` by default).
    fn get_update_packet(
        &self,
    ) -> Option<mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket> {
        None
    }
    /// Whether this type has a server ticker (`getTicker`); the level calls [`BlockEntity::server_tick`] each tick if so.
    fn is_ticking(&self) -> bool {
        false
    }
    /// `getTicker(level, ..)` for block entities whose ticker depends on the level (the daylight detector).
    fn is_ticking_in(&self, _level: &crate::server::level::ServerLevel) -> bool {
        self.is_ticking()
    }
    fn server_tick(&mut self, _level: &mut crate::server::level::ServerLevel) {}
    /// A ticker that works on the level and the position instead of the taken-out block entity: for block entities
    /// whose tick changes their own block while neighbours read the block entity (sculk sensor, catalyst, shrieker,
    /// creaking heart). When present it is used instead of [`BlockEntity::server_tick`].
    fn level_ticker(&self) -> Option<fn(&mut crate::server::level::ServerLevel, BlockPos)> {
        None
    }
    /// The `VibrationSystem.Data` of block entities that implement `VibrationSystem` (sculk sensors, shriekers).
    fn vibration_data_mut(
        &mut self,
    ) -> Option<&mut crate::world::level::gameevent::vibrations::VibrationData> {
        None
    }
    /// `setBlockState(state)`.
    fn set_block_state(&mut self, state: BlockState) {
        self.base_mut().block_state = state;
    }
    /// `clearRemoved()`.
    fn clear_removed(&mut self) {
        self.base_mut().removed = false;
    }
    /// `isValidBlockState(state)`: `getType().isValid(state)`.
    fn is_valid_block_state(&self, state: BlockState) -> bool {
        self.get_type().data().valid_blocks.contains(&state.block())
    }
    /// `preRemoveSideEffects(pos, state)`: called (server side) before the block entity is removed because its block
    /// was replaced. A `Container` unpacks its loot table (vanilla's `getItem`) and drops its contents; the entity is out
    /// of the chunk while this runs.
    fn pre_remove_side_effects(
        &mut self,
        level: &mut crate::server::level::ServerLevel,
        pos: BlockPos,
        _state: BlockState,
    ) {
        self.unpack_loot_table(level, None);
        if let Some(container) = self.as_container() {
            crate::world::containers::drop_contents_at(level, pos, container);
        }
    }
    /// `EntityBlock.getListener(level, blockEntity)`: the game event listener of this block entity, if it has one.
    fn game_event_listener(&self) -> Option<crate::world::level::gameevent::BlockEntityListener> {
        None
    }
    // ==== [Blocks:TechnicalA] hooks ====
    /// `BlockEntity.triggerEvent(b0, b1)` (reached through `BaseEntityBlock.triggerEvent`, see
    /// `helpers::base_entity_trigger_event`); the entity is out of the chunk while this runs.
    fn trigger_event(
        &mut self,
        _level: &mut crate::server::level::ServerLevel,
        _b0: i32,
        _b1: i32,
    ) -> bool {
        false
    }
    /// `setRemoved()` for types whose override uses the level (beacon deactivate sound): called by the level when the
    /// entity is removed from its chunk (`Level.removeBlockEntity`, replaced entry); the entity is out of the chunk.
    fn set_removed_in_level(&mut self, _level: &mut crate::server::level::ServerLevel) {
        self.set_removed();
    }

    // ==== [Blocks:RedstoneDevices] ====
    /// `SkullBlockEntity.getNoteBlockSound()`: the sound a note block plays below this head (`None` for every other type).
    fn get_note_block_sound(&self) -> Option<mc_core::ident::Identifier> {
        None
    }

    // ==== [Blocks:Dispensers] RandomizableContainer hooks ====
    /// `RandomizableContainer.unpackLootTable(player)` (no-op for block entities without a loot table); the block entity is
    /// out of its chunk while this runs. See `randomizable_container`.
    fn unpack_loot_table(
        &mut self,
        _level: &mut crate::server::level::ServerLevel,
        _player: Option<&mut crate::world::entity::ServerPlayer>,
    ) {
    }
    /// `RandomizableContainer.getLootTable() != null`: an unpack would roll a table.
    fn has_loot_table(&self) -> bool {
        false
    }
    /// `RandomizableContainer.setLootTable(table, seed)` (no-op for other block entities).
    fn set_loot_table(&mut self, _table: mc_core::ident::Identifier, _seed: i64) {}
    /// `BrushableBlockEntity.brush(gameTime, level, user, direction, brush)`: `None` for every other block entity. The block
    /// entity is out of its chunk (see [`brush_block`]).
    fn try_brush(
        &mut self,
        _game_time: i64,
        _level: &mut crate::server::level::ServerLevel,
        _user: &mut crate::world::entity::ServerPlayer,
        _direction: mc_core::direction::Direction,
        _brush: &crate::world::item::ItemStack,
    ) -> Option<bool> {
        None
    }

    /// The `Container` view of this block entity (vanilla `this instanceof Container`); menus reach block-entity
    /// containers through `ContainerKey::BlockEntity(pos)`.
    fn as_container(&self) -> Option<&dyn crate::world::container::Container> {
        None
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn crate::world::container::Container> {
        None
    }
    /// The container a menu slot reads and writes: `as_container`, except where the menu holds an anonymous container
    /// that the block entity is not (`LecternBlockEntity.bookAccess`), which hoppers and droppers must not see.
    fn menu_container(&self) -> Option<&dyn crate::world::container::Container> {
        self.as_container()
    }
    fn menu_container_mut(&mut self) -> Option<&mut dyn crate::world::container::Container> {
        self.as_container_mut()
    }
    /// The `ContainerData` this block entity hands its menu (`dataAccess`: furnace progress, brewing time, beacon
    /// levels, lectern page, ...).
    fn container_data(&self) -> Option<&dyn crate::world::inventory::ContainerData> {
        None
    }
    fn container_data_mut(&mut self) -> Option<&mut dyn crate::world::inventory::ContainerData> {
        None
    }
    /// `Container.startOpen(user)` / `stopOpen(user)` of a block-entity container (chest open counter, sounds, block
    /// events); called by the menus, the block entity is out of its chunk while this runs.
    /// Set by the block entity's `Container::set_item`/`set_changed`/`remove_item` when vanilla would touch the level
    /// there (`this.level`: recipe lookup, `setChanged()` -> `blockEntityChanged` + comparator update); the menu
    /// resolver then calls [`BlockEntity::container_accessed`] right after the container operation.
    /// `blockEntity instanceof MenuProvider`: the provider that opens this block entity's menu (it addresses the block
    /// entity by position, so it stays valid after the block entity is taken out).
    fn menu_provider(&self) -> Option<Box<dyn crate::world::inventory::MenuProvider>> {
        None
    }
    /// `AbstractFurnaceBlockEntity.awardUsedRecipesAndPopExperience(player)` minus the player half: pops the experience of
    /// the recipes used at `position` (the player's), forgets them and returns the recipes to award (the caller unlocks
    /// them and fires the `recipe_crafted` trigger over the container's stacks). Block entities without recipes used
    /// return none.
    fn award_used_recipes_and_pop_experience(
        &mut self,
        _level: &mut crate::server::level::ServerLevel,
        _position: mc_core::phys::Vec3,
    ) -> Vec<crate::world::item::crafting::RecipeHolder> {
        Vec::new()
    }
    fn container_pending(&self) -> bool {
        false
    }
    /// Does the level-dependent part of the container operation that just ran (see `container_pending`); clears the flag.
    fn container_accessed(&mut self, _level: &mut crate::server::level::ServerLevel) {}
    fn start_open(
        &mut self,
        _level: &mut crate::server::level::ServerLevel,
        _user: &ContainerUser,
    ) {
    }
    fn stop_open(&mut self, _level: &mut crate::server::level::ServerLevel, _user: &ContainerUser) {
    }
    // ==== [Blocks:TechnicalB] ====
    /// `loadAdditional(input)` of a block entity that is attached to a level (vanilla's `this.level != null`): command blocks
    /// schedule their tick when `auto` turns on, structure blocks update their block state. The entity is out of the chunk
    /// while this runs (use [`crate::server::level::ServerLevel::load_block_entity_in_level`]), so a block state change must
    /// not be made here: return it and the caller sets it (flags 2) once the entity is back. The default is `loadAdditional`,
    /// which is all the types that ignore their level need.
    fn load_additional_in_level(
        &mut self,
        _level: &mut crate::server::level::ServerLevel,
        tag: &CompoundTag,
    ) -> Option<BlockState> {
        self.load_additional(tag);
        None
    }

    // ==== [Blocks:Spawners] hooks ====
    /// This entity as a `Spawner` (`SpawnerBlockEntity`, `TrialSpawnerBlockEntity`) for spawn eggs / `/data`.
    fn as_spawner_mut(&mut self) -> Option<&mut dyn Spawner> {
        None
    }
}

/// `ValueInput.read(key, codec)`: the decoded value, or the partial value of a failed decode (`resultOrPartial`).
pub fn read_codec<A: 'static>(tag: &CompoundTag, key: &str, codec: &mc_dfu::Codec<A>) -> Option<A> {
    mc_dfu::parse_nbt(codec, tag.get(key)?).result_or_partial()
}

/// A registered block entity type implementation.
pub struct BlockEntityRegistration {
    pub ty: BlockEntityType,
    pub create: fn(BlockPos, BlockState) -> Box<dyn BlockEntity>,
}

/// All block entity implementations (`BlockEntityType.create`).
#[distributed_slice]
pub static BLOCK_ENTITY_TYPES: [BlockEntityRegistration];

/// `BlockEntityType.create(pos, state)`: `None` when no implementation is registered yet.
pub fn create_block_entity(
    ty: BlockEntityType,
    pos: BlockPos,
    state: BlockState,
) -> Option<Box<dyn BlockEntity>> {
    BLOCK_ENTITY_TYPES
        .iter()
        .find(|r| r.ty == ty)
        .map(|r| (r.create)(pos, state))
}

/// `EntityBlock.newBlockEntity(pos, state)` for a block entity block: the registered implementation of the block entity
/// type that is valid for the state's block (`None` while that type has no implementation).
pub fn new_block_entity(pos: BlockPos, state: BlockState) -> Option<Box<dyn BlockEntity>> {
    // `MovingPistonBlock.newBlockEntity` returns null: the entity is attached by `setBlockEntity` (`PistonBaseBlock`)
    if state.block() == mc_data::Block::MOVING_PISTON {
        return None;
    }
    static BY_BLOCK: std::sync::OnceLock<
        std::collections::HashMap<mc_data::Block, BlockEntityType>,
    > = std::sync::OnceLock::new();
    let map = BY_BLOCK.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for ty in BlockEntityType::iter() {
            for block in ty.data().valid_blocks {
                map.insert(*block, ty);
            }
        }
        map
    });
    create_block_entity(*map.get(&state.block())?, pos, state)
}

/// `BlockEntity.setChanged(level, pos, state)` (the static form the `BlockEntity.setChanged()` instance method delegates
/// to): marks the chunk unsaved and notifies comparators when the state is not air. A block entity that is out of its chunk
/// running its own code is invisible to the comparators it updates, so the notification waits for the put-back (see
/// [`deferred`]); vanilla's comparators read the live entity.
pub fn set_changed(
    level: &mut crate::server::level::ServerLevel,
    pos: BlockPos,
    state: BlockState,
) {
    if level.is_block_entity_taken(pos) {
        deferred::push(move |level| set_changed(level, pos, state));
        return;
    }
    level.block_entity_changed(pos);
    if !state.is_air() {
        level.update_neighbour_for_output_signal(pos, state.block());
    }
}

/// `BrushItem.onUseTick`'s `level.getBlockEntity(pos) instanceof BrushableBlockEntity be && be.brush(...)`: whether the
/// block at `pos` was fully brushed by this stroke.
pub fn brush_block(
    level: &mut crate::server::level::ServerLevel,
    pos: BlockPos,
    user: &mut crate::world::entity::ServerPlayer,
    direction: mc_core::direction::Direction,
    brush: &crate::world::item::ItemStack,
) -> bool {
    let game_time = crate::world::level::LevelAccessor::get_game_time(level);
    level
        .with_block_entity(pos, |level, be| {
            be.try_brush(game_time, level, user, direction, brush)
        })
        .flatten()
        .unwrap_or(false)
}
