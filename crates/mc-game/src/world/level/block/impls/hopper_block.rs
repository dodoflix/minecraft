//! `net.minecraft.world.level.block.HopperBlock`: `rotate` / `mirror` of the `facing` property.
use super::prelude::*;

pub struct HopperBlock;

impl BlockBehaviour for HopperBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING_HOPPER, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING_HOPPER, mirror)
    }
}

register_block_class!("HopperBlock", |_| Box::new(HopperBlock));
