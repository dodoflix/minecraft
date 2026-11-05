//! `net.minecraft.world.level.block.MultifaceBlock` (resin clump; base of `MultifaceSpreadeableBlock`: glow lichen, sculk vein).
use super::prelude::*;
use mc_data::props::BooleanProperty;

pub struct MultifaceBlock;

/// `MultifaceBlock.getFaceProperty`.
pub fn get_face_property(face_direction: Direction) -> BooleanProperty {
    helpers::pipe_property(face_direction)
}

/// `MultifaceBlock.hasFace`.
pub fn has_face(state: BlockState, face_direction: Direction) -> bool {
    state.try_get(get_face_property(face_direction)).unwrap_or(false)
}

/// `MultifaceBlock.hasAnyFace`.
pub fn has_any_face(state: BlockState) -> bool {
    Direction::ALL.into_iter().any(|d| has_face(state, d))
}

/// `MultifaceBlock.availableFaces` (an `EnumSet`: ascending ordinal order).
pub fn available_faces(state: BlockState) -> Vec<Direction> {
    if !helpers::is_instance_of(state, "MultifaceBlock") {
        return Vec::new();
    }
    Direction::ALL.into_iter().filter(|d| has_face(state, *d)).collect()
}

/// `MultifaceBlock.unpack`.
pub fn unpack(data: i8) -> Vec<Direction> {
    Direction::ALL.into_iter().filter(|d| data & (1i8.wrapping_shl(d.ordinal() as u32)) > 0).collect()
}

/// `MultifaceBlock.pack`.
pub fn pack(directions: &[Direction]) -> i8 {
    directions.iter().fold(0i8, |code, d| code | 1i8.wrapping_shl(d.ordinal() as u32))
}

/// `MultifaceBlock.canAttachTo(level, pos, directionTowardsNeighbour)`.
pub fn can_attach_to_pos(level: &dyn BlockGetter, pos: BlockPos, direction_towards_neighbour: Direction) -> bool {
    let neighbour_pos = pos.relative(direction_towards_neighbour);
    let block_state = level.get_block_state(neighbour_pos);
    can_attach_to(level, direction_towards_neighbour, neighbour_pos, block_state)
}

/// `MultifaceBlock.canAttachTo(level, directionTowardsNeighbour, neighbourPos, neighbourState)`.
pub fn can_attach_to(level: &dyn BlockGetter, direction_towards_neighbour: Direction, neighbour_pos: BlockPos, neighbour_state: BlockState) -> bool {
    helpers::is_face_full(&neighbour_state.get_block_support_shape(level, neighbour_pos), direction_towards_neighbour.get_opposite())
        || helpers::is_face_full(&neighbour_state.get_collision_shape(level, neighbour_pos), direction_towards_neighbour.get_opposite())
}

fn remove_face(state: BlockState, property: BooleanProperty) -> BlockState {
    let new_state = state.set(property, false);
    if has_any_face(new_state) { new_state } else { air() }
}

/// `MultifaceBlock.isValidStateForPlacement` (`isFaceSupported` is true for every face of every multiface block).
pub fn is_valid_state_for_placement(this: Block, level: &dyn BlockGetter, old_state: BlockState, placement_pos: BlockPos, placement_direction: Direction) -> bool {
    if !old_state.is_block(this) || !has_face(old_state, placement_direction) {
        let neighbour_pos = placement_pos.relative(placement_direction);
        can_attach_to(level, placement_direction, neighbour_pos, level.get_block_state(neighbour_pos))
    } else {
        false
    }
}

/// `MultifaceBlock.getStateForPlacement(oldState, level, placementPos, placementDirection)`.
pub fn get_state_for_placement(this: Block, old_state: BlockState, level: &dyn BlockGetter, placement_pos: BlockPos, placement_direction: Direction) -> Option<BlockState> {
    if !is_valid_state_for_placement(this, level, old_state, placement_pos, placement_direction) {
        return None;
    }
    let new_state = if old_state.is_block(this) {
        old_state
    } else if old_state.get_fluid_state().is_source_of_type(mc_data::fluid::Fluid::WATER) {
        this.default_state().set(props::WATERLOGGED, true)
    } else {
        this.default_state()
    };
    Some(new_state.set(get_face_property(placement_direction), true))
}

fn map_directions(state: BlockState, mapping: impl Fn(Direction) -> Direction) -> BlockState {
    let mut new_state = state;
    for direction in Direction::ALL {
        new_state = new_state.set(get_face_property(mapping(direction)), state.get(get_face_property(direction)));
    }
    new_state
}

impl BlockBehaviour for MultifaceBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if !has_any_face(state) {
            air()
        } else if has_face(state, direction) && !can_attach_to(&*level, direction, neighbour_pos, neighbour_state) {
            remove_face(state, get_face_property(direction))
        } else {
            state
        }
    });
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let mut has_at_least_one_face = false;
        for direction in Direction::ALL {
            if has_face(state, direction) {
                if !can_attach_to_pos(level, pos, direction) {
                    return false;
                }
                has_at_least_one_face = true;
            }
        }
        has_at_least_one_face
    }
    // `canRotate`, `canMirrorX`, `canMirrorZ` are true for every block (all six faces are supported)
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        map_directions(state, |d| rotation.rotate(d))
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        map_directions(state, |d| mirror.mirror(d))
    }
}

register_block_class!("MultifaceBlock", |_| Box::new(MultifaceBlock));
