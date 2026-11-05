//! `net.minecraft.world.level.block.MovingPistonBlock` (`piston` package).
use super::prelude::*;

pub struct MovingPistonBlock;

impl BlockBehaviour for MovingPistonBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("MovingPistonBlock", |_| Box::new(MovingPistonBlock));
