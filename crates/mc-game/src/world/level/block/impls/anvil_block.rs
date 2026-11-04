//! `net.minecraft.world.level.block.AnvilBlock`: `rotate` of the `facing` property.
use super::prelude::*;

pub struct AnvilBlock;

impl BlockBehaviour for AnvilBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
}

register_block_class!("AnvilBlock", |_| Box::new(AnvilBlock));
