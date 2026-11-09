//! `net.minecraft.world.level.block.VineBlock`.
use super::multiface_block;
use super::prelude::*;
use crate::server::level::ServerLevel;
use mc_core::random::RandomSource;
use mc_data::props::BooleanProperty;

pub struct VineBlock;

/// `VineBlock.getPropertyForFace` (`PROPERTY_BY_DIRECTION` has no `down` entry).
pub fn get_property_for_face(direction: Direction) -> BooleanProperty {
    helpers::pipe_property(direction)
}

/// `VineBlock.isAcceptableNeighbour`.
pub fn is_acceptable_neighbour(
    level: &dyn BlockGetter,
    neighbour_pos: BlockPos,
    direction_to_neighbour: Direction,
) -> bool {
    multiface_block::can_attach_to(
        level,
        direction_to_neighbour,
        neighbour_pos,
        level.get_block_state(neighbour_pos),
    )
}

const FACE_PROPERTIES: [BooleanProperty; 5] = [
    props::UP,
    props::NORTH,
    props::EAST,
    props::SOUTH,
    props::WEST,
];

fn count_faces(state: BlockState) -> usize {
    FACE_PROPERTIES
        .into_iter()
        .filter(|p| state.get(*p))
        .count()
}

fn has_faces(state: BlockState) -> bool {
    count_faces(state) > 0
}

fn has_horizontal_connection(state: BlockState) -> bool {
    state.get(props::NORTH)
        || state.get(props::EAST)
        || state.get(props::SOUTH)
        || state.get(props::WEST)
}

/// `copyRandomFaces`.
fn copy_random_faces(
    from: BlockState,
    mut to: BlockState,
    random: &mut dyn RandomSource,
) -> BlockState {
    for direction in Direction::HORIZONTAL {
        if random.next_boolean() {
            let property = get_property_for_face(direction);
            if from.get(property) {
                to = to.set(property, true);
            }
        }
    }
    to
}

/// `canSpread`: at most 4 vines in the 9x3x9 box around `pos`.
fn can_spread(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let min_pos = pos.offset(-4, -1, -4);
    let max_pos = pos.offset(4, 1, 4);
    BlockPos::between_closed(&min_pos, &max_pos)
        .filter(|p| level.get_block_state(*p).is_block(Block::VINE))
        .take(5)
        .count()
        <= 4
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
        state = state.set(
            props::UP,
            is_acceptable_neighbour(level, above_pos, Direction::Down),
        );
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

/// `VineBlock.randomTick`.
fn random_tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if !level.spread_vines() || level.random().next_int_bounded(4) != 0 {
        return;
    }
    let test_direction = Direction::get_random(level.random());
    let above_pos = pos.above();
    if test_direction.get_axis().is_horizontal()
        && !state.get(get_property_for_face(test_direction))
    {
        if !can_spread(&*level, pos) {
            return;
        }
        let test_pos = pos.relative(test_direction);
        let edge_state = level.get_block_state(test_pos);
        if edge_state.is_air() {
            let cw_direction = test_direction.get_clockwise();
            let ccw_direction = test_direction.get_counter_clockwise();
            let cw_has_connecting_face = state.get(get_property_for_face(cw_direction));
            let ccw_has_connecting_face = state.get(get_property_for_face(ccw_direction));
            let cw_test_pos = test_pos.relative(cw_direction);
            let ccw_test_pos = test_pos.relative(ccw_direction);
            let vine = Block::VINE.default_state();
            if cw_has_connecting_face && is_acceptable_neighbour(&*level, cw_test_pos, cw_direction)
            {
                level.set_block_flags(
                    test_pos,
                    vine.set(get_property_for_face(cw_direction), true),
                    2,
                );
            } else if ccw_has_connecting_face
                && is_acceptable_neighbour(&*level, ccw_test_pos, ccw_direction)
            {
                level.set_block_flags(
                    test_pos,
                    vine.set(get_property_for_face(ccw_direction), true),
                    2,
                );
            } else {
                let opposite = test_direction.get_opposite();
                if cw_has_connecting_face
                    && level.is_empty_block(cw_test_pos)
                    && is_acceptable_neighbour(&*level, pos.relative(cw_direction), opposite)
                {
                    level.set_block_flags(
                        cw_test_pos,
                        vine.set(get_property_for_face(opposite), true),
                        2,
                    );
                } else if ccw_has_connecting_face
                    && level.is_empty_block(ccw_test_pos)
                    && is_acceptable_neighbour(&*level, pos.relative(ccw_direction), opposite)
                {
                    level.set_block_flags(
                        ccw_test_pos,
                        vine.set(get_property_for_face(opposite), true),
                        2,
                    );
                } else if (level.random().next_float() as f64) < 0.05
                    && is_acceptable_neighbour(&*level, test_pos.above(), Direction::Up)
                {
                    level.set_block_flags(test_pos, vine.set(props::UP, true), 2);
                }
            }
        } else if is_acceptable_neighbour(&*level, test_pos, test_direction) {
            level.set_block_flags(
                pos,
                state.set(get_property_for_face(test_direction), true),
                2,
            );
        }
    } else {
        if test_direction == Direction::Up && pos.y() < level.get_max_y() {
            if can_support_at_face(&*level, pos, test_direction) {
                level.set_block_flags(pos, state.set(props::UP, true), 2);
                return;
            }
            if level.is_empty_block(above_pos) {
                if !can_spread(&*level, pos) {
                    return;
                }
                let mut above_state = state;
                for direction in Direction::HORIZONTAL {
                    if level.random().next_boolean()
                        || !is_acceptable_neighbour(
                            &*level,
                            above_pos.relative(direction),
                            direction,
                        )
                    {
                        above_state = above_state.set(get_property_for_face(direction), false);
                    }
                }
                if has_horizontal_connection(above_state) {
                    level.set_block_flags(above_pos, above_state, 2);
                }
                return;
            }
        }
        if pos.y() > level.get_min_y() {
            let below_pos = pos.below();
            let below_state = level.get_block_state(below_pos);
            if below_state.is_air() || below_state.is_block(Block::VINE) {
                let before = if below_state.is_air() {
                    Block::VINE.default_state()
                } else {
                    below_state
                };
                let after = copy_random_faces(state, before, level.random());
                if before != after && has_horizontal_connection(after) {
                    level.set_block_flags(below_pos, after, 2);
                }
            }
        }
    }
}

impl BlockBehaviour for VineBlock {
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        random_tick(state, level, pos);
    }
    fn can_be_replaced(
        &self,
        state: BlockState,
        context: &crate::world::item::BlockPlaceContext,
    ) -> bool {
        // vanilla reads the clicked position's state whatever state is asked (`bed`/`door` ask about their neighbours)
        let clicked_state = context.get_clicked_state();
        if clicked_state.is_block(Block::VINE) {
            count_faces(clicked_state) < FACE_PROPERTIES.len()
        } else {
            crate::world::level::block::PlainBlock.can_be_replaced(state, context)
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &crate::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        let clicked_state = level.get_block_state(context.get_clicked_pos());
        let clicked_vine = clicked_state.is_block(block);
        let result = if clicked_vine {
            clicked_state
        } else {
            block.default_state()
        };
        for direction in context.get_nearest_looking_directions() {
            if direction != Direction::Down {
                let face = get_property_for_face(direction);
                let face_occupied = clicked_vine && clicked_state.get(face);
                if !face_occupied
                    && can_support_at_face(&*level, context.get_clicked_pos(), direction)
                {
                    return Some(result.set(face, true));
                }
            }
        }
        clicked_vine.then_some(result)
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        has_faces(get_updated_state(state, level, pos))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Down {
                state
            } else {
                let block_state = get_updated_state(state, &*level, pos);
                if !has_faces(block_state) {
                    air()
                } else {
                    block_state
                }
            }
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_sides(
            state,
            rotation,
            [props::NORTH, props::EAST, props::SOUTH, props::WEST],
        )
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_sides(
            state,
            mirror,
            [props::NORTH, props::EAST, props::SOUTH, props::WEST],
        )
    }
}

register_block_class!("VineBlock", |_| Box::new(VineBlock));
