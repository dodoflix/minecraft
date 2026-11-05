//! `TheEndPortalBlockEntity`: no saved data of its own.

use super::{BLOCK_ENTITY_TYPES, BlockEntity, BlockEntityData, BlockEntityRegistration};
use linkme::distributed_slice;
use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;
use mc_data::block::BlockState;

pub struct TheEndPortalBlockEntity {
    base: BlockEntityData,
}

impl TheEndPortalBlockEntity {
    pub fn new(pos: BlockPos, state: BlockState) -> Self {
        Self { base: BlockEntityData::new(BlockEntityType::END_PORTAL, pos, state) }
    }
}

impl BlockEntity for TheEndPortalBlockEntity {
    block_entity_base!();
}

#[distributed_slice(BLOCK_ENTITY_TYPES)]
static END_PORTAL: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::END_PORTAL, create: |pos, state| Box::new(TheEndPortalBlockEntity::new(pos, state)) };
