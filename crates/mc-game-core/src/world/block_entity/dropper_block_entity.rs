//! `DropperBlockEntity`: a `DispenserBlockEntity` of type `DROPPER` (title `container.dropper`).

use super::dispenser_block_entity::DispenserBlockEntity;
use super::{BLOCK_ENTITY_TYPES, BlockEntityRegistration};
use mc_data::BlockEntityType;

#[linkme::distributed_slice(BLOCK_ENTITY_TYPES)]
static DROPPER: BlockEntityRegistration =
    BlockEntityRegistration { ty: BlockEntityType::DROPPER, create: |pos, state| Box::new(DispenserBlockEntity::new(BlockEntityType::DROPPER, pos, state)) };
