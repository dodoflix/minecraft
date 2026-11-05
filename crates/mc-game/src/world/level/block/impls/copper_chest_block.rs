//! `net.minecraft.world.level.block.CopperChestBlock` (extends `ChestBlock`; also the weathering variant).
use super::prelude::*;
use super::chest_block::{self, get_connected_direction};
use mc_data::enums::ChestType;

pub struct CopperChestBlock;

/// `CopperChestBlock.chestCanConnectTo`.
fn chest_can_connect_to(state: BlockState) -> bool {
    state.is_tag("copper_chests") && state.has_property(props::CHEST_TYPE)
}

impl BlockBehaviour for CopperChestBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let block_state = chest_block::update_shape(state, level, pos, direction, neighbour_state, &chest_can_connect_to);
        if chest_can_connect_to(neighbour_state) {
            let chest_type = block_state.get(props::CHEST_TYPE);
            if chest_type != ChestType::Single && get_connected_direction(block_state) == direction {
                return neighbour_state.block().default_state().with_properties_of(block_state);
            }
        }
        block_state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CopperChestBlock", |_| Box::new(CopperChestBlock));
