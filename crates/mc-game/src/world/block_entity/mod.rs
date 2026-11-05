//! `net.minecraft.world.level.block.entity`: block entities are `Box<dyn BlockEntity>` stored in the level's chunk
//! (same take-out-while-ticking pattern as entities). Concrete types register a factory through
//! [`BLOCK_ENTITY_TYPES`] from the module that implements them; unknown types are kept as raw NBT by mc-world so
//! nothing is lost on save.

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
        self.base_mut().components = tag
            .get("components")
            .and_then(|t| mc_dfu::parse_nbt(&mc_protocol::components::DataComponentMap::codec(), t).into_result().ok())
            .unwrap_or_default();
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
