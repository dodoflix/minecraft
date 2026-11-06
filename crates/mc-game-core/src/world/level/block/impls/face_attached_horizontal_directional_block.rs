//! `net.minecraft.world.level.block.FaceAttachedHorizontalDirectionalBlock` (extends `HorizontalDirectionalBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use mc_data::enums::AttachFace;

pub struct FaceAttachedHorizontalDirectionalBlock;

/// `FaceAttachedHorizontalDirectionalBlock.canAttach`.
pub fn can_attach(level: &dyn LevelReader, pos: BlockPos, direction: Direction) -> bool {
    let relative = pos.relative(direction);
    level
        .get_block_state(relative)
        .is_face_sturdy_at(level, relative, direction.get_opposite())
}

/// `FaceAttachedHorizontalDirectionalBlock.getConnectedDirection`.
pub fn get_connected_direction(state: BlockState) -> Direction {
    match state.get(props::ATTACH_FACE) {
        AttachFace::Ceiling => Direction::Down,
        AttachFace::Floor => Direction::Up,
        AttachFace::Wall => state.get(props::HORIZONTAL_FACING),
    }
}

/// `canSurvive`.
pub fn can_survive(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    can_attach(level, pos, get_connected_direction(state).get_opposite())
}

/// `getStateForPlacement`.
pub fn get_state_for_placement(
    block: Block,
    level: &dyn LevelReader,
    context: &BlockPlaceContext,
) -> Option<BlockState> {
    for direction in context.get_nearest_looking_directions() {
        let state = if direction.get_axis() == Axis::Y {
            block
                .default_state()
                .set(
                    props::ATTACH_FACE,
                    if direction == Direction::Up {
                        AttachFace::Ceiling
                    } else {
                        AttachFace::Floor
                    },
                )
                .set(props::HORIZONTAL_FACING, context.get_horizontal_direction())
        } else {
            block
                .default_state()
                .set(props::ATTACH_FACE, AttachFace::Wall)
                .set(props::HORIZONTAL_FACING, direction.get_opposite())
        };
        if state.can_survive(level, context.get_clicked_pos()) {
            return Some(state);
        }
    }
    None
}

/// `updateShape`.
pub fn update_shape(
    state: BlockState,
    level: &dyn LevelReader,
    pos: BlockPos,
    direction_to_neighbour: Direction,
) -> BlockState {
    if get_connected_direction(state).get_opposite() == direction_to_neighbour
        && !state.can_survive(level, pos)
    {
        air()
    } else {
        state
    }
}

impl BlockBehaviour for FaceAttachedHorizontalDirectionalBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive(state, level, pos)
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        get_state_for_placement(block, &*level, context)
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

register_block_class!("FaceAttachedHorizontalDirectionalBlock", |_| Box::new(
    FaceAttachedHorizontalDirectionalBlock
));
