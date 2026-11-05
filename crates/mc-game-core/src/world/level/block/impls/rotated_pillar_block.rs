//! `net.minecraft.world.level.block.RotatedPillarBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;

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

/// `getStateForPlacement` (also `InfestedRotatedPillarBlock`).
pub fn get_state_for_placement(block: Block, context: &BlockPlaceContext) -> BlockState {
    block.default_state().set(props::AXIS, context.get_clicked_face().get_axis())
}

impl BlockBehaviour for RotatedPillarBlock {
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(get_state_for_placement(block, context))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }
}

register_block_class!("RotatedPillarBlock", |_| Box::new(RotatedPillarBlock));
