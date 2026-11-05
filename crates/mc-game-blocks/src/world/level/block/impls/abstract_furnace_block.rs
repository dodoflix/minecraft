//! `net.minecraft.world.level.block.AbstractFurnaceBlock`: `rotate` / `mirror` of the `facing` property.
use super::prelude::*;

pub struct AbstractFurnaceBlock;

impl BlockBehaviour for AbstractFurnaceBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("AbstractFurnaceBlock", |_| Box::new(AbstractFurnaceBlock));
