//! `net.minecraft.world.level.block.ChainBlock` (extends `RotatedPillarBlock`).
use super::prelude::*;
use super::rotated_pillar_block::rotate_pillar;

pub struct ChainBlock;

impl BlockBehaviour for ChainBlock {
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
