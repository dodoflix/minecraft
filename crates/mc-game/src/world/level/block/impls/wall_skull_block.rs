//! `net.minecraft.world.level.block.WallSkullBlock` (extends `AbstractSkullBlock`).
use super::prelude::*;

pub struct WallSkullBlock;

impl BlockBehaviour for WallSkullBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("WallSkullBlock", |_| Box::new(WallSkullBlock));
