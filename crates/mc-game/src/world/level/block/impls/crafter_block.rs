//! `net.minecraft.world.level.block.CrafterBlock` (`orientation` rotates / mirrors like the jigsaw block).
use super::prelude::*;

pub struct CrafterBlock;

/// `mc_data`'s `FrontAndTop` (the property value) as the `mc_core` one that has the geometry (same constant order).
fn core_orientation(state: BlockState) -> mc_core::direction::FrontAndTop {
    mc_core::direction::FrontAndTop::ALL[state.get(props::ORIENTATION).ordinal()]
}

fn with_orientation(state: BlockState, orientation: mc_core::direction::FrontAndTop) -> BlockState {
    state.set(props::ORIENTATION, mc_data::enums::FrontAndTop::ALL[orientation.ordinal()])
}

impl BlockBehaviour for CrafterBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        with_orientation(state, rotation.rotation().rotate_front_and_top(core_orientation(state)).expect("rotated jigsaw orientation"))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        with_orientation(state, mirror.rotation().rotate_front_and_top(core_orientation(state)).expect("mirrored jigsaw orientation"))
    }
}

/// `CrafterBlock.getFrontFacing(state)`.
pub fn get_front_facing(state: BlockState) -> Direction {
    core_orientation(state).front()
}

/// `CrafterBlock.getTopFacing(state)`.
pub fn get_top_facing(state: BlockState) -> Direction {
    core_orientation(state).top()
}

register_block_class!("CrafterBlock", |_| Box::new(CrafterBlock));
