//! `net.minecraft.world.level.block.ShulkerBoxBlock`: `rotate` / `mirror` of the `facing` property.
use super::prelude::*;

pub struct ShulkerBoxBlock;

impl BlockBehaviour for ShulkerBoxBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("ShulkerBoxBlock", |_| Box::new(ShulkerBoxBlock));
