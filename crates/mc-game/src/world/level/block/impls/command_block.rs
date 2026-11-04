//! `net.minecraft.world.level.block.CommandBlock`: `rotate` / `mirror` of the `facing` property.
use super::prelude::*;

pub struct CommandBlock;

impl BlockBehaviour for CommandBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("CommandBlock", |_| Box::new(CommandBlock));
