//! `net.minecraft.world.level.block.DispenserBlock`: `rotate` / `mirror` of the `facing` property.
use super::prelude::*;

pub struct DispenserBlock;

impl BlockBehaviour for DispenserBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("DispenserBlock", |_| Box::new(DispenserBlock));
