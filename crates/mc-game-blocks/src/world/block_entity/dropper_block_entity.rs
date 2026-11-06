//! `DropperBlockEntity`: a `DispenserBlockEntity` of type `DROPPER` (title `container.dropper`).

use mc_data::BlockEntityType;
use mc_game_core::world::block_entity::dispenser_block_entity::DispenserBlockEntity;
use mc_game_core::world::block_entity::{BLOCK_ENTITY_TYPES, BlockEntityRegistration};

#[linkme::distributed_slice(BLOCK_ENTITY_TYPES)]
static DROPPER: BlockEntityRegistration = BlockEntityRegistration {
    ty: BlockEntityType::DROPPER,
    create: |pos, state| {
        Box::new(DispenserBlockEntity::new(
            BlockEntityType::DROPPER,
            pos,
            state,
        ))
    },
};
