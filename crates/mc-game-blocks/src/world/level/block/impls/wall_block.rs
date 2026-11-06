//! `net.minecraft.world.level.block.WallBlock`.
use mc_core::phys::shapes::{BooleanOp, Shapes, VoxelShape};
use mc_data::enums::WallSide;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};
use std::collections::HashMap;
use std::sync::LazyLock;

pub struct WallBlock;

/// `WallBlock.TEST_SHAPE_POST`.
static TEST_SHAPE_POST: LazyLock<VoxelShape> =
    LazyLock::new(|| mc_game_core::world::level::shapes::column(2.0, 0.0, 16.0));
/// `WallBlock.TEST_SHAPES_WALL`.
static TEST_SHAPES_WALL: LazyLock<HashMap<Direction, VoxelShape>> = LazyLock::new(|| {
    Shapes::rotate_horizontal(&mc_game_core::world::level::shapes::box_z_xy(
        2.0, 16.0, 0.0, 9.0,
    ))
});

fn connects_to(state: BlockState, face_solid: bool, direction: Direction) -> bool {
    let connected_fence_gate = helpers::is_instance_of(state, "FenceGateBlock")
        && super::fence_gate_block::connects_to_direction(state, direction);
    state.is_tag("walls")
        || (!helpers::is_exception_for_connection(state) && face_solid)
        || helpers::is_instance_of(state, "IronBarsBlock")
        || connected_fence_gate
}

fn is_connected(state: BlockState, wall: mc_data::props::EnumProperty<WallSide>) -> bool {
    state.get(wall) != WallSide::None
}

fn is_covered(above_shape: &VoxelShape, test_shape: &VoxelShape) -> bool {
    !Shapes::join_is_not_empty(test_shape, above_shape, BooleanOp::ONLY_FIRST)
}

fn make_wall_state(
    connects_to_side: bool,
    above_shape: &VoxelShape,
    test_shape: &VoxelShape,
) -> WallSide {
    if connects_to_side {
        if is_covered(above_shape, test_shape) {
            WallSide::Tall
        } else {
            WallSide::Low
        }
    } else {
        WallSide::None
    }
}

fn update_sides(
    state: BlockState,
    north: bool,
    east: bool,
    south: bool,
    west: bool,
    above_shape: &VoxelShape,
) -> BlockState {
    state
        .set(
            props::NORTH_WALL,
            make_wall_state(north, above_shape, &TEST_SHAPES_WALL[&Direction::North]),
        )
        .set(
            props::EAST_WALL,
            make_wall_state(east, above_shape, &TEST_SHAPES_WALL[&Direction::East]),
        )
        .set(
            props::SOUTH_WALL,
            make_wall_state(south, above_shape, &TEST_SHAPES_WALL[&Direction::South]),
        )
        .set(
            props::WEST_WALL,
            make_wall_state(west, above_shape, &TEST_SHAPES_WALL[&Direction::West]),
        )
}

fn should_raise_post(
    state: BlockState,
    top_neighbour: BlockState,
    above_shape: &VoxelShape,
) -> bool {
    let top_neighbour_has_post =
        helpers::is_instance_of(top_neighbour, "WallBlock") && top_neighbour.get(props::UP);
    if top_neighbour_has_post {
        return true;
    }
    let north_wall = state.get(props::NORTH_WALL);
    let south_wall = state.get(props::SOUTH_WALL);
    let east_wall = state.get(props::EAST_WALL);
    let west_wall = state.get(props::WEST_WALL);
    let south_none = south_wall == WallSide::None;
    let west_none = west_wall == WallSide::None;
    let east_none = east_wall == WallSide::None;
    let north_none = north_wall == WallSide::None;
    let has_corner = (north_none && south_none && west_none && east_none)
        || north_none != south_none
        || west_none != east_none;
    if has_corner {
        true
    } else {
        let has_high_wall = (north_wall == WallSide::Tall && south_wall == WallSide::Tall)
            || (east_wall == WallSide::Tall && west_wall == WallSide::Tall);
        if has_high_wall {
            false
        } else {
            top_neighbour.is_tag("wall_post_override") || is_covered(above_shape, &TEST_SHAPE_POST)
        }
    }
}

fn update_shape_full(
    level: &dyn LevelReader,
    state: BlockState,
    top_pos: BlockPos,
    top_neighbour: BlockState,
    north: bool,
    east: bool,
    south: bool,
    west: bool,
) -> BlockState {
    let above_shape = top_neighbour
        .get_collision_shape(level, top_pos)
        .get_face_shape(Direction::Down);
    let sides_updated_state = update_sides(state, north, east, south, west, &above_shape);
    sides_updated_state.set(
        props::UP,
        should_raise_post(sides_updated_state, top_neighbour, &above_shape),
    )
}

fn top_update(
    level: &dyn LevelReader,
    state: BlockState,
    top_pos: BlockPos,
    top_neighbour: BlockState,
) -> BlockState {
    let north = is_connected(state, props::NORTH_WALL);
    let east = is_connected(state, props::EAST_WALL);
    let south = is_connected(state, props::SOUTH_WALL);
    let west = is_connected(state, props::WEST_WALL);
    update_shape_full(
        level,
        state,
        top_pos,
        top_neighbour,
        north,
        east,
        south,
        west,
    )
}

fn side_update(
    level: &dyn LevelReader,
    pos: BlockPos,
    state: BlockState,
    neighbour_pos: BlockPos,
    neighbour: BlockState,
    direction: Direction,
) -> BlockState {
    let opposite = direction.get_opposite();
    let side = |d: Direction, wall| {
        if direction == d {
            connects_to(
                neighbour,
                neighbour.is_face_sturdy_at(level, neighbour_pos, opposite),
                opposite,
            )
        } else {
            is_connected(state, wall)
        }
    };
    let is_north_connected = side(Direction::North, props::NORTH_WALL);
    let is_east_connected = side(Direction::East, props::EAST_WALL);
    let is_south_connected = side(Direction::South, props::SOUTH_WALL);
    let is_west_connected = side(Direction::West, props::WEST_WALL);
    let above = pos.above();
    let above_state = level.get_block_state(above);
    update_shape_full(
        level,
        state,
        above,
        above_state,
        is_north_connected,
        is_east_connected,
        is_south_connected,
        is_west_connected,
    )
}

impl BlockBehaviour for WallBlock {
    mc_game_core::simple_waterlogged!();
    fn is_pathfindable(
        &self,
        _state: BlockState,
        _ty: mc_data::block::PathComputationType,
    ) -> bool {
        false
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut mc_game_core::server::level::ServerLevel,
        context: &mc_game_core::world::item::BlockPlaceContext,
    ) -> Option<BlockState> {
        let pos = context.get_clicked_pos();
        let replaced_fluid_state = level.get_fluid_state(pos);
        let mut connected = [false; 4];
        for (slot, (direction, opposite)) in [
            (Direction::North, Direction::South),
            (Direction::East, Direction::West),
            (Direction::South, Direction::North),
            (Direction::West, Direction::East),
        ]
        .into_iter()
        .enumerate()
        {
            let neighbour_pos = pos.relative(direction);
            let neighbour = level.get_block_state(neighbour_pos);
            connected[slot] = connects_to(
                neighbour,
                neighbour.is_face_sturdy_at(&*level, neighbour_pos, opposite),
                opposite,
            );
        }
        let top_pos = pos.above();
        let top_state = level.get_block_state(top_pos);
        let state = block.default_state().set(
            props::WATERLOGGED,
            replaced_fluid_state.get_type() == mc_data::fluid::Fluid::WATER,
        );
        Some(update_shape_full(
            &*level,
            state,
            top_pos,
            top_state,
            connected[0],
            connected[1],
            connected[2],
            connected[3],
        ))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            if direction == Direction::Down {
                state
            } else if direction == Direction::Up {
                top_update(&*level, state, neighbour_pos, neighbour_state)
            } else {
                side_update(
                    &*level,
                    pos,
                    state,
                    neighbour_pos,
                    neighbour_state,
                    direction,
                )
            }
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_sides(
            state,
            rotation,
            [
                props::NORTH_WALL,
                props::EAST_WALL,
                props::SOUTH_WALL,
                props::WEST_WALL,
            ],
        )
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_sides(
            state,
            mirror,
            [
                props::NORTH_WALL,
                props::EAST_WALL,
                props::SOUTH_WALL,
                props::WEST_WALL,
            ],
        )
    }
}

register_block_class!("WallBlock", |_| Box::new(WallBlock));
