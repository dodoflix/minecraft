//! `net.minecraft.world.level.block.WallTorchBlock` (extends `TorchBlock`).
use super::prelude::*;

pub struct WallTorchBlock;

/// `WallTorchBlock.canSurvive(level, pos, facing)`.
pub fn can_survive(level: &dyn LevelReader, pos: BlockPos, facing: Direction) -> bool {
    let relative_pos = pos.relative(facing.get_opposite());
    level.get_block_state(relative_pos).is_face_sturdy_at(level, relative_pos, facing)
}

/// `updateShape` shared with `RedstoneWallTorchBlock`.
pub fn update_shape(state: BlockState, level: &dyn LevelReader, pos: BlockPos, direction_to_neighbour: Direction) -> BlockState {
    if direction_to_neighbour.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(level, pos) { air() } else { state }
}

impl BlockBehaviour for WallTorchBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut crate::server::level::ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let mut state = block.default_state();
        let pos = context.get_clicked_pos();
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal() {
                state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
                if state.can_survive(&*level, pos) {
                    return Some(state);
                }
            }
        }
        None
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive(level, pos, state.get(props::HORIZONTAL_FACING))
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
        update_shape(state, &*level, pos, direction_to_neighbour)
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("WallTorchBlock", |_| Box::new(WallTorchBlock));
