//! `net.minecraft.world.level.block.BellBlock`.
use super::prelude::*;
use mc_data::enums::BellAttachType;
use super::face_attached_horizontal_directional_block as face_attached;

pub struct BellBlock;

fn get_connected_direction(state: BlockState) -> Direction {
    match state.get(props::BELL_ATTACHMENT) {
        BellAttachType::Floor => Direction::Up,
        BellAttachType::Ceiling => Direction::Down,
        _ => state.get(props::HORIZONTAL_FACING).get_opposite(),
    }
}

impl BlockBehaviour for BellBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let connection_dir = get_connected_direction(state).get_opposite();
        if connection_dir == Direction::Up {
            helpers::can_support_center(level, pos.above(), Direction::Down)
        } else {
            face_attached::can_attach(level, pos, connection_dir)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let attachment = state.get(props::BELL_ATTACHMENT);
        let connected_direction = get_connected_direction(state).get_opposite();
        if connected_direction == direction && !state.can_survive(&*level, pos) && attachment != BellAttachType::DoubleWall {
            return air();
        }
        if direction.get_axis() == state.get(props::HORIZONTAL_FACING).get_axis() {
            if attachment == BellAttachType::DoubleWall && !neighbour_state.is_face_sturdy_at(&*level, neighbour_pos, direction) {
                return state.set(props::BELL_ATTACHMENT, BellAttachType::SingleWall).set(props::HORIZONTAL_FACING, direction.get_opposite());
            }
            if attachment == BellAttachType::SingleWall
                && connected_direction.get_opposite() == direction
                && neighbour_state.is_face_sturdy_at(&*level, neighbour_pos, state.get(props::HORIZONTAL_FACING))
            {
                return state.set(props::BELL_ATTACHMENT, BellAttachType::DoubleWall);
            }
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BellBlock", |_| Box::new(BellBlock));
