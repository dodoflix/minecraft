//! `net.minecraft.world.level.block.PistonBaseBlock` (`piston` package).
use super::prelude::*;

pub struct PistonBaseBlock;

impl BlockBehaviour for PistonBaseBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("PistonBaseBlock", |_| Box::new(PistonBaseBlock));
