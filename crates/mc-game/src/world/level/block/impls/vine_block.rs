//! `net.minecraft.world.level.block.VineBlock`.
use super::prelude::*;
use super::multiface_block;
use mc_data::props::BooleanProperty;

pub struct VineBlock;

/// `VineBlock.getPropertyForFace` (`PROPERTY_BY_DIRECTION` has no `down` entry).
pub fn get_property_for_face(direction: Direction) -> BooleanProperty {
    helpers::pipe_property(direction)
}

/// `VineBlock.isAcceptableNeighbour`.
pub fn is_acceptable_neighbour(level: &dyn BlockGetter, neighbour_pos: BlockPos, direction_to_neighbour: Direction) -> bool {
    multiface_block::can_attach_to(level, direction_to_neighbour, neighbour_pos, level.get_block_state(neighbour_pos))
}

fn has_faces(state: BlockState) -> bool {
    [props::UP, props::NORTH, props::EAST, props::SOUTH, props::WEST].into_iter().any(|p| state.get(p))
}

fn can_support_at_face(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> bool {
    if direction == Direction::Down {
        return false;
    }
    let relative = pos.relative(direction);
    if is_acceptable_neighbour(level, relative, direction) {
        true
    } else if direction.get_axis() == Axis::Y {
        false
    } else {
        let property = get_property_for_face(direction);
        let above_state = level.get_block_state(pos.above());
        above_state.is_block(Block::VINE) && above_state.get(property)
    }
}

fn get_updated_state(mut state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    let above_pos = pos.above();
    if state.get(props::UP) {
        state = state.set(props::UP, is_acceptable_neighbour(level, above_pos, Direction::Down));
    }
    let mut above_state: Option<BlockState> = None;
    for direction in Direction::HORIZONTAL {
        let property = get_property_for_face(direction);
        if state.get(property) {
            let mut can_support = can_support_at_face(level, pos, direction);
            if !can_support {
                let above = *above_state.get_or_insert_with(|| level.get_block_state(above_pos));
                can_support = above.is_block(Block::VINE) && above.get(property);
            }
            state = state.set(property, can_support);
        }
    }
    state
}

impl BlockBehaviour for VineBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        has_faces(get_updated_state(state, level, pos))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down {
            state
        } else {
            let block_state = get_updated_state(state, &*level, pos);
            if !has_faces(block_state) { air() } else { block_state }
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_sides(state, rotation, [props::NORTH, props::EAST, props::SOUTH, props::WEST])
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_sides(state, mirror, [props::NORTH, props::EAST, props::SOUTH, props::WEST])
    }
}

register_block_class!("VineBlock", |_| Box::new(VineBlock));
