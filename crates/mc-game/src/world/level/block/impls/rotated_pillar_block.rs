//! `net.minecraft.world.level.block.RotatedPillarBlock`.
use super::prelude::*;

pub struct RotatedPillarBlock;

/// `RotatedPillarBlock.rotatePillar`.
pub fn rotate_pillar(state: BlockState, rotation: Rotation) -> BlockState {
    match rotation {
        Rotation::Counterclockwise90 | Rotation::Clockwise90 => match state.get(props::AXIS) {
            Axis::X => state.set(props::AXIS, Axis::Z),
            Axis::Z => state.set(props::AXIS, Axis::X),
            _ => state,
        },
        _ => state,
    }
}

impl BlockBehaviour for RotatedPillarBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }
}

register_block_class!("RotatedPillarBlock", |_| Box::new(RotatedPillarBlock));
