//! `net.minecraft.world.level.block.SkullBlock` (extends `AbstractSkullBlock`).
use super::prelude::*;

pub struct SkullBlock;

impl BlockBehaviour for SkullBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        state.set(props::ROTATION_16, rotation.rotate_steps(state.get(props::ROTATION_16), 16))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::ROTATION_16, mirror.mirror_steps(state.get(props::ROTATION_16), 16))
    }
}

register_block_class!("SkullBlock", |_| Box::new(SkullBlock));
