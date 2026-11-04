//! `net.minecraft.world.level.block.ChiseledBookShelfBlock`: `rotate` / `mirror` of the `facing` property.
use super::prelude::*;

pub struct ChiseledBookShelfBlock;

impl BlockBehaviour for ChiseledBookShelfBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("ChiseledBookShelfBlock", |_| Box::new(ChiseledBookShelfBlock));
