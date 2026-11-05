//! `net.minecraft.world.level.block.RodBlock` (extends `DirectionalBlock`; end rod, lightning rod).
use super::prelude::*;

pub struct RodBlock;

impl BlockBehaviour for RodBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::FACING, mirror.mirror(state.get(props::FACING)))
    }
}

register_block_class!("RodBlock", |_| Box::new(RodBlock));
