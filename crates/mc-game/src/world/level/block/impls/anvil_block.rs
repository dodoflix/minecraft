//! `net.minecraft.world.level.block.AnvilBlock` (extends `FallingBlock`).
use super::prelude::*;
use super::falling_block;

pub struct AnvilBlock;

impl BlockBehaviour for AnvilBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        falling_block::update_shape(state, level, pos, 2)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
}

register_block_class!("AnvilBlock", |_| Box::new(AnvilBlock));
