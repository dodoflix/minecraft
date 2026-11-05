//! `net.minecraft.world.level.block.EndRodBlock` (extends `RodBlock`: shape/rotate/mirror/pathfinding come from it).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use super::rod_block::RodBlock;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_data::block::PathComputationType;

pub struct EndRodBlock;

impl BlockBehaviour for EndRodBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let clicked_face = context.get_clicked_face();
        let state = level.get_block_state(context.get_clicked_pos().relative(clicked_face.get_opposite()));
        Some(if state.is_block(block) && state.get(props::FACING) == clicked_face {
            block.default_state().set(props::FACING, clicked_face.get_opposite())
        } else {
            block.default_state().set(props::FACING, clicked_face)
        })
    }
    fn is_pathfindable(&self, state: BlockState, ty: PathComputationType) -> bool {
        RodBlock.is_pathfindable(state, ty)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        RodBlock.rotate(state, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        RodBlock.mirror(state, mirror)
    }
}

register_block_class!("EndRodBlock", |_| Box::new(EndRodBlock));
