//! `net.minecraft.world.level.block.entity`: block entities are `Box<dyn BlockEntity>` stored in the level's chunk
//! (same take-out-while-ticking pattern as entities). Concrete types register a factory through
//! [`BLOCK_ENTITY_TYPES`] from the module that implements them (one file per vanilla class); types without an
//! implementation are kept as raw NBT by mc-world so nothing is lost on save.

/// The accessors every implementation of [`BlockEntity`] needs (the struct keeps its [`BlockEntityData`] in `base`).
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

pub mod base_container;
mod calibrated_sculk_sensor_block_entity;
mod comparator_block_entity;
mod creaking_heart_block_entity;
mod daylight_detector_block_entity;
mod potent_sulfur_block_entity;
mod sculk_catalyst_block_entity;
mod sculk_sensor_block_entity;
mod sculk_shrieker_block_entity;
mod the_end_gateway_block_entity;
mod the_end_portal_block_entity;

pub use comparator_block_entity::ComparatorBlockEntity;
pub use creaking_heart_block_entity::CreakingHeartBlockEntity;
pub use daylight_detector_block_entity::DaylightDetectorBlockEntity;
pub use potent_sulfur_block_entity::PotentSulfurBlockEntity;
pub use sculk_catalyst_block_entity::SculkCatalystBlockEntity;
pub use sculk_sensor_block_entity::SculkSensorBlockEntity;
pub use sculk_shrieker_block_entity::SculkShriekerBlockEntity;
pub use the_end_gateway_block_entity::TheEndGatewayBlockEntity;
pub use the_end_portal_block_entity::TheEndPortalBlockEntity;

use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;
use mc_nbt::CompoundTag;
use std::any::Any;

/// Fields every block entity has (vanilla `BlockEntity`).
#[derive(Clone, Debug)]
pub struct BlockEntityData {
    pub ty: BlockEntityType,
    pub pos: BlockPos,
    pub block_state: BlockState,
    pub removed: bool,
    /// `BlockEntity.components` (`DataComponentMap.EMPTY` until set).
    pub components: mc_protocol::components::DataComponentMap,
}

impl BlockEntityData {
    pub fn new(ty: BlockEntityType, pos: BlockPos, block_state: BlockState) -> Self {
        Self { ty, pos, block_state, removed: false, components: Default::default() }
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
        if let Ok(components) = mc_dfu::encode_nbt(&mc_protocol::components::DataComponentMap::codec(), &self.base().components).into_result() {
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
        self.base_mut().components = read_codec(tag, "components", &mc_protocol::components::DataComponentMap::codec()).unwrap_or_default();
    }
    // ==== [Loot] additions (BlockEntity trait methods) ====
    /// `collectImplicitComponents(builder)`: the components a block entity derives from its own state (container contents,
    /// custom name, banner patterns, ...), written on top of `components` by `collectComponents`.
    fn collect_implicit_components(&self, _builder: &mut mc_protocol::components::map::DataComponentMapBuilder) {}
    /// `collectComponents()`: the stored `components` plus the implicit ones.
    fn collect_components(&self) -> mc_protocol::components::DataComponentMap {
        let mut builder = mc_protocol::components::DataComponentMap::builder().add_all(&self.base().components);
        self.collect_implicit_components(&mut builder);
        builder.build()
    }
    /// `Nameable.getCustomName()` (block entities that have one).
    fn get_custom_name(&self) -> Option<mc_text::Component> {
        None
    }
    /// `this instanceof SlotProvider` (containers readable through slot ranges).
    fn as_slot_provider(&self) -> Option<&dyn crate::world::item::slot::SlotProvider> {
        None
    }
    /// `applyComponentsFromItemStack(stack)`: block entities that read item components (custom name, container
    /// contents, ...) override this; the default ignores them.
    fn apply_components_from_item_stack(&mut self, _stack: &crate::world::item::ItemStack) {}
    /// `getUpdateTag`: the NBT sent to clients in chunk packets / block entity data packets (default empty).
    fn get_update_tag(&self) -> CompoundTag {
        CompoundTag::new()
    }
    /// `getUpdatePacket()`: the `ClientboundBlockEntityDataPacket` for clients (`null` by default).
    fn get_update_packet(&self) -> Option<mc_protocol::packets::play::level::ClientboundBlockEntityDataPacket> {
        None
    }
    /// Whether this type has a server ticker (`getTicker`); the level calls [`BlockEntity::server_tick`] each tick if so.
    fn is_ticking(&self) -> bool {
        false
    }
    fn server_tick(&mut self, _level: &mut crate::server::level::ServerLevel) {}
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
    /// was replaced (containers drop their contents here). The entity is out of the chunk while this runs.
    fn pre_remove_side_effects(&mut self, _level: &mut crate::server::level::ServerLevel, _pos: BlockPos, _state: BlockState) {}
    /// `EntityBlock.getListener(level, blockEntity)`: the game event listener of this block entity, if it has one.
    fn game_event_listener(&mut self) -> Option<&mut dyn crate::world::level::gameevent::GameEventListener> {
        None
    }
    // ==== [Blocks:TechnicalA] hooks ====
    /// `BlockEntity.triggerEvent(b0, b1)` (reached through `BaseEntityBlock.triggerEvent`, see
    /// `helpers::base_entity_trigger_event`); the entity is out of the chunk while this runs.
    fn trigger_event(&mut self, _level: &mut crate::server::level::ServerLevel, _b0: i32, _b1: i32) -> bool {
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

    /// The `Container` view of this block entity (vanilla `this instanceof Container`); menus reach block-entity
    /// containers through `ContainerKey::BlockEntity(pos)`.
    fn as_container(&self) -> Option<&dyn crate::world::container::Container> {
        None
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn crate::world::container::Container> {
        None
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
    fn container_pending(&self) -> bool {
        false
    }
    /// Does the level-dependent part of the container operation that just ran (see `container_pending`); clears the flag.
    fn container_accessed(&mut self, _level: &mut crate::server::level::ServerLevel) {}
    fn start_open(&mut self, _level: &mut crate::server::level::ServerLevel, _user: &ContainerUser) {}
    fn stop_open(&mut self, _level: &mut crate::server::level::ServerLevel, _user: &ContainerUser) {}
}

/// `ValueInput.read(key, codec)`: the decoded value, or the partial value of a failed decode (`resultOrPartial`).
pub(crate) fn read_codec<A: 'static>(tag: &CompoundTag, key: &str, codec: &mc_dfu::Codec<A>) -> Option<A> {
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
pub fn create_block_entity(ty: BlockEntityType, pos: BlockPos, state: BlockState) -> Option<Box<dyn BlockEntity>> {
    BLOCK_ENTITY_TYPES.iter().find(|r| r.ty == ty).map(|r| (r.create)(pos, state))
}

/// `EntityBlock.newBlockEntity(pos, state)` for a block entity block: the registered implementation of the block entity
/// type that is valid for the state's block (`None` while that type has no implementation).
pub fn new_block_entity(pos: BlockPos, state: BlockState) -> Option<Box<dyn BlockEntity>> {
    static BY_BLOCK: std::sync::OnceLock<std::collections::HashMap<mc_data::Block, BlockEntityType>> = std::sync::OnceLock::new();
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
/// to): marks the chunk unsaved and notifies comparators when the state is not air.
pub fn set_changed(level: &mut crate::server::level::ServerLevel, pos: BlockPos, state: BlockState) {
    level.block_entity_changed(pos);
    if !state.is_air() {
        level.update_neighbour_for_output_signal(pos, state.block());
    }
}
