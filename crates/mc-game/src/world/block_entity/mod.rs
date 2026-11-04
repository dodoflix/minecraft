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
}

impl BlockEntityData {
    pub fn new(ty: BlockEntityType, pos: BlockPos, block_state: BlockState) -> Self {
        Self { ty, pos, block_state, removed: false }
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
    /// `getUpdateTag`: the NBT sent to clients in chunk packets / block entity data packets (default empty).
    fn get_update_tag(&self) -> CompoundTag {
        CompoundTag::new()
    }
    /// Whether this type has a server ticker (`getTicker`); the level calls [`BlockEntity::server_tick`] each tick if so.
    fn is_ticking(&self) -> bool {
        false
    }
    fn server_tick(&mut self, _level: &mut crate::server::level::ServerLevel) {}
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
