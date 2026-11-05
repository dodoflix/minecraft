//! `net.minecraft.world.level.block.MossyCarpetBlock` (pale moss carpet).
use super::prelude::*;
use super::multiface_block;
use mc_data::enums::WallSide;
use mc_data::props::EnumProperty;

pub struct MossyCarpetBlock;

/// `MossyCarpetBlock.getPropertyForFace` (horizontal faces only).
pub fn get_property_for_face(direction: Direction) -> Option<EnumProperty<WallSide>> {
    match direction {
        Direction::North => Some(props::NORTH_WALL),
        Direction::East => Some(props::EAST_WALL),
        Direction::South => Some(props::SOUTH_WALL),
        Direction::West => Some(props::WEST_WALL),
        _ => None,
    }
}

fn has_faces(state: BlockState) -> bool {
    state.get(props::BOTTOM) || Direction::HORIZONTAL.into_iter().any(|d| state.get(get_property_for_face(d).unwrap()) != WallSide::None)
}

fn can_support_at_face(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> bool {
    direction != Direction::Up && multiface_block::can_attach_to_pos(level, pos, direction)
}

fn get_updated_state(mut state: BlockState, level: &dyn BlockGetter, pos: BlockPos, mut create_sides: bool) -> BlockState {
    let mut above_state: Option<BlockState> = None;
    let mut below_state: Option<BlockState> = None;
    create_sides |= state.get(props::BOTTOM);
    for direction in Direction::HORIZONTAL {
        let property = get_property_for_face(direction).unwrap();
        let mut side = if can_support_at_face(level, pos, direction) {
            if create_sides { WallSide::Low } else { state.get(property) }
        } else {
            WallSide::None
        };
        if side == WallSide::Low {
            let above = *above_state.get_or_insert_with(|| level.get_block_state(pos.above()));
            if above.is_block(Block::PALE_MOSS_CARPET) && above.get(property) != WallSide::None && !above.get(props::BOTTOM) {
                side = WallSide::Tall;
            }
            if !state.get(props::BOTTOM) {
                let below = *below_state.get_or_insert_with(|| level.get_block_state(pos.below()));
                if below.is_block(Block::PALE_MOSS_CARPET) && below.get(property) == WallSide::None {
                    side = WallSide::None;
                }
            }
        }
        state = state.set(property, side);
    }
    state
}

/// First half of `createTopperWithSideChance`: the (still untested) state above and the state it would replace;
/// `None` when no topper is possible.
fn topper_candidate(level: &dyn BlockGetter, pos: BlockPos) -> Option<(BlockState, BlockState)> {
    let above = pos.above();
    let above_previous_state = level.get_block_state(above);
    let is_mossy_carpet_above = above_previous_state.is_block(Block::PALE_MOSS_CARPET);
    if (!is_mossy_carpet_above || !above_previous_state.get(props::BOTTOM)) && (is_mossy_carpet_above || above_previous_state.can_be_replaced()) {
        let no_carpet_base_state = Block::PALE_MOSS_CARPET.default_state().set(props::BOTTOM, false);
        Some((get_updated_state(no_carpet_base_state, level, pos.above(), true), above_previous_state))
    } else {
        None
    }
}

/// Second half of `createTopperWithSideChance`: every present side survives when `side_survival_test` says so.
fn topper_with_side_chance(candidate: Option<(BlockState, BlockState)>, side_survival_test: &mut dyn FnMut() -> bool) -> BlockState {
    let Some((mut above_state, above_previous_state)) = candidate else { return air() };
    for direction in Direction::HORIZONTAL {
        let property = get_property_for_face(direction).unwrap();
        if above_state.get(property) != WallSide::None && !side_survival_test() {
            above_state = above_state.set(property, WallSide::None);
        }
    }
    if has_faces(above_state) && above_state != above_previous_state { above_state } else { air() }
}

/// `MossyCarpetBlock.placeAt(level, pos, random, updateType)` with `random` = `level.getRandom()`.
pub fn place_at(level: &mut dyn LevelAccessor, pos: BlockPos, update_type: u32) {
    let simple_carpet_layer = Block::PALE_MOSS_CARPET.default_state();
    let adjusted_carpet_layer = get_updated_state(simple_carpet_layer, &*level, pos, true);
    level.set_block_flags(pos, adjusted_carpet_layer, update_type);
    let candidate = topper_candidate(&*level, pos);
    let state = topper_with_side_chance(candidate, &mut || level.random().next_boolean());
    if !state.is_air() {
        level.set_block_flags(pos.above(), state, update_type);
        let update_bottom_carpet = get_updated_state(adjusted_carpet_layer, &*level, pos, true);
        level.set_block_flags(pos, update_bottom_carpet, update_type);
    }
}

impl BlockBehaviour for MossyCarpetBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = level.get_block_state(pos.below());
        if state.get(props::BOTTOM) { !below_state.is_air() } else { below_state.is_block(state.block()) && below_state.get(props::BOTTOM) }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            air()
        } else {
            let block_state = get_updated_state(state, &*level, pos, false);
            if !has_faces(block_state) { air() } else { block_state }
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_sides(state, rotation, [props::NORTH_WALL, props::EAST_WALL, props::SOUTH_WALL, props::WEST_WALL])
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_sides(state, mirror, [props::NORTH_WALL, props::EAST_WALL, props::SOUTH_WALL, props::WEST_WALL])
    }
}

register_block_class!("MossyCarpetBlock", |_| Box::new(MossyCarpetBlock));
