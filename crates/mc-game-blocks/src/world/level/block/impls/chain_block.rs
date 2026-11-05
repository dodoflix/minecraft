//! `net.minecraft.world.level.block.ChainBlock` (extends `RotatedPillarBlock`).
use super::prelude::*;
use super::rotated_pillar_block::rotate_pillar;

pub struct ChainBlock;

impl BlockBehaviour for ChainBlock {
    crate::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut crate::server::level::ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let is_water_source = level.get_fluid_state(context.get_clicked_pos()).get_type() == mc_data::fluid::Fluid::WATER;
        Some(super::rotated_pillar_block::get_state_for_placement(block, context).set(props::WATERLOGGED, is_water_source))
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        false
    }
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        _direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }
}

register_block_class!("ChainBlock", |_| Box::new(ChainBlock));
