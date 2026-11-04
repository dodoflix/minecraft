//! `net.minecraft.world.level.block.BarrelBlock`: `rotate` / `mirror` of the `facing` property.
use super::prelude::*;

pub struct BarrelBlock;

impl BlockBehaviour for BarrelBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("BarrelBlock", |_| Box::new(BarrelBlock));
