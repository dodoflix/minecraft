//! `net.minecraft.world.level.block.SkullBlock` (extends `AbstractSkullBlock`); also `PlayerHeadBlock`.
use super::abstract_skull_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::rotation_segment;
use crate::world::level::redstone::Orientation;
use mc_data::block::PathComputationType;

pub struct SkullBlock;

impl BlockBehaviour for SkullBlock {
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let state = abstract_skull_block::get_state_for_placement(block, &*level, context.get_clicked_pos());
        Some(state.set(props::ROTATION_16, rotation_segment::convert_to_segment(context.get_rotation())))
    }
    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        abstract_skull_block::neighbor_changed(state, level, pos);
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        state.set(props::ROTATION_16, rotation.rotate_steps(state.get(props::ROTATION_16), 16))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::ROTATION_16, mirror.mirror_steps(state.get(props::ROTATION_16), 16))
    }
}

register_block_class!("SkullBlock", |_| Box::new(SkullBlock));
