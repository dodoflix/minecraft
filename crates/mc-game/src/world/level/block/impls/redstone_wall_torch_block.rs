//! `net.minecraft.world.level.block.RedstoneWallTorchBlock` (extends `RedstoneTorchBlock`).
use super::prelude::*;
use super::wall_torch_block;

pub struct RedstoneWallTorchBlock;

impl BlockBehaviour for RedstoneWallTorchBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        wall_torch_block::can_survive(level, pos, state.get(props::HORIZONTAL_FACING))
    }
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        wall_torch_block::update_shape(state, &*level, pos, direction_to_neighbour)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("RedstoneWallTorchBlock", |_| Box::new(RedstoneWallTorchBlock));
