//! `net.minecraft.world.level.block.RedstoneWireBlock` (`redstone_wire`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::item::{BlockPlaceContext, InteractionResult};
use crate::world::level::redstone::default_redstone_wire_evaluator::DefaultRedstoneWireEvaluator;
use crate::world::level::redstone::experimental_redstone_wire_evaluator::ExperimentalRedstoneWireEvaluator;
use crate::world::level::redstone::redstone_wire_evaluator::RedstoneWireEvaluator;
use crate::world::level::redstone::{ExperimentalRedstoneUtils, Orientation};
use crate::world::level::signal_getter::get_best_neighbor_signal;
use mc_data::enums::RedstoneSide;
use mc_data::props::EnumProperty;
use mc_world::level_storage::FeatureFlagSet;
use std::cell::Cell;

pub struct RedstoneWireBlock;

thread_local! {
    /// `RedStoneWireBlock.shouldSignal` (a field of the singleton block; one game thread per level here).
    static SHOULD_SIGNAL: Cell<bool> = const { Cell::new(true) };
}

fn should_signal() -> bool {
    SHOULD_SIGNAL.get()
}

/// `RedStoneWireBlock.getBlockSignal(level, pos)`: the best signal of the neighbours other than wires.
pub fn get_block_signal(level: &ServerLevel, pos: BlockPos) -> i32 {
    SHOULD_SIGNAL.set(false);
    let block_signal = get_best_neighbor_signal(level, pos);
    SHOULD_SIGNAL.set(true);
    block_signal
}

fn use_experimental_evaluator(level: &ServerLevel) -> bool {
    level
        .enabled_features()
        .contains_all(FeatureFlagSet::REDSTONE_EXPERIMENTS)
}

/// `RedStoneWireBlock.updatePowerStrength(level, pos, state, orientation, shapeUpdateWiresAroundInitialPosition)`.
fn update_power_strength(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    orientation: Option<Orientation>,
    shape_update_wires_around_initial_position: bool,
) {
    if use_experimental_evaluator(level) {
        ExperimentalRedstoneWireEvaluator::new().update_power_strength(
            level,
            pos,
            state,
            orientation,
            shape_update_wires_around_initial_position,
        );
    } else {
        DefaultRedstoneWireEvaluator.update_power_strength(
            level,
            pos,
            state,
            orientation,
            shape_update_wires_around_initial_position,
        );
    }
}

fn check_corner_change_at(level: &mut ServerLevel, pos: BlockPos) {
    if BlockGetter::get_block_state(level, pos).is_block(Block::REDSTONE_WIRE) {
        level.update_neighbors_at(pos, Block::REDSTONE_WIRE);
        for direction in Direction::ALL {
            level.update_neighbors_at(pos.relative(direction), Block::REDSTONE_WIRE);
        }
    }
}

fn update_neighbors_of_neighboring_wires(level: &mut ServerLevel, pos: BlockPos) {
    for direction in Direction::HORIZONTAL {
        check_corner_change_at(level, pos.relative(direction));
    }
    for direction in Direction::HORIZONTAL {
        let target = pos.relative(direction);
        if BlockGetter::get_block_state(level, target).is_redstone_conductor() {
            check_corner_change_at(level, target.above());
        } else {
            check_corner_change_at(level, target.below());
        }
    }
}

fn updates_on_shape_change(
    level: &mut ServerLevel,
    pos: BlockPos,
    old_state: BlockState,
    new_state: BlockState,
) {
    let orientation =
        ExperimentalRedstoneUtils::initial_orientation(level, None, Some(Direction::Up));
    for direction in Direction::HORIZONTAL {
        let relative_pos = pos.relative(direction);
        if is_connected(old_state.get(side_property(direction)))
            != is_connected(new_state.get(side_property(direction)))
            && BlockGetter::get_block_state(level, relative_pos).is_redstone_conductor()
        {
            level.update_neighbors_at_except_from_facing(
                relative_pos,
                new_state.block(),
                direction.get_opposite(),
                ExperimentalRedstoneUtils::with_front(orientation, direction),
            );
        }
    }
}

fn side_property(direction: Direction) -> EnumProperty<RedstoneSide> {
    match direction {
        Direction::North => props::NORTH_REDSTONE,
        Direction::East => props::EAST_REDSTONE,
        Direction::South => props::SOUTH_REDSTONE,
        _ => props::WEST_REDSTONE,
    }
}

fn is_connected(side: RedstoneSide) -> bool {
    side != RedstoneSide::None
}

fn is_cross(state: BlockState) -> bool {
    Direction::HORIZONTAL
        .into_iter()
        .all(|d| is_connected(state.get(side_property(d))))
}

fn is_dot(state: BlockState) -> bool {
    Direction::HORIZONTAL
        .into_iter()
        .all(|d| !is_connected(state.get(side_property(d))))
}

/// `RedStoneWireBlock.crossState`.
fn cross_state() -> BlockState {
    Block::REDSTONE_WIRE
        .default_state()
        .set(props::NORTH_REDSTONE, RedstoneSide::Side)
        .set(props::EAST_REDSTONE, RedstoneSide::Side)
        .set(props::SOUTH_REDSTONE, RedstoneSide::Side)
        .set(props::WEST_REDSTONE, RedstoneSide::Side)
}

/// `RedStoneWireBlock.shouldConnectTo(level, pos)`.
fn should_connect_to_pos(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    level
        .get_block_state(pos)
        .should_redstone_wire_connect_to(level, pos, None)
}

fn can_survive_on(
    level: &dyn BlockGetter,
    relative_pos: BlockPos,
    relative_state: BlockState,
) -> bool {
    relative_state.is_face_sturdy_at(level, relative_pos, Direction::Up)
        || relative_state.is_block(Block::HOPPER)
}

fn get_connecting_side(
    level: &dyn BlockGetter,
    pos: BlockPos,
    direction: Direction,
    can_connect_up: bool,
) -> RedstoneSide {
    let relative_pos = pos.relative(direction);
    let relative_state = level.get_block_state(relative_pos);
    if can_connect_up {
        let is_placeable_above = helpers::is_instance_of(relative_state, "TrapDoorBlock")
            || can_survive_on(level, relative_pos, relative_state);
        if is_placeable_above && should_connect_to_pos(level, relative_pos.above()) {
            if relative_state.is_face_sturdy_at(level, relative_pos, direction.get_opposite()) {
                return RedstoneSide::Up;
            }
            return RedstoneSide::Side;
        }
    }
    if !relative_state.should_redstone_wire_connect_to(level, relative_pos, Some(direction))
        && (relative_state.is_redstone_conductor()
            || !should_connect_to_pos(level, relative_pos.below()))
    {
        RedstoneSide::None
    } else {
        RedstoneSide::Side
    }
}

fn get_missing_connections(
    level: &dyn BlockGetter,
    mut state: BlockState,
    pos: BlockPos,
) -> BlockState {
    let can_connect_up = !level.get_block_state(pos.above()).is_redstone_conductor();
    for direction in Direction::HORIZONTAL {
        if !is_connected(state.get(side_property(direction))) {
            state = state.set(
                side_property(direction),
                get_connecting_side(level, pos, direction, can_connect_up),
            );
        }
    }
    state
}

fn get_connection_state(
    level: &dyn BlockGetter,
    mut state: BlockState,
    pos: BlockPos,
) -> BlockState {
    let was_dot = is_dot(state);
    state = get_missing_connections(
        level,
        Block::REDSTONE_WIRE
            .default_state()
            .set(props::POWER, state.get(props::POWER)),
        pos,
    );
    if was_dot && is_dot(state) {
        return state;
    }
    let north = is_connected(state.get(props::NORTH_REDSTONE));
    let south = is_connected(state.get(props::SOUTH_REDSTONE));
    let east = is_connected(state.get(props::EAST_REDSTONE));
    let west = is_connected(state.get(props::WEST_REDSTONE));
    let north_south_empty = !north && !south;
    let east_west_empty = !east && !west;
    if !west && north_south_empty {
        state = state.set(props::WEST_REDSTONE, RedstoneSide::Side);
    }
    if !east && north_south_empty {
        state = state.set(props::EAST_REDSTONE, RedstoneSide::Side);
    }
    if !north && east_west_empty {
        state = state.set(props::NORTH_REDSTONE, RedstoneSide::Side);
    }
    if !south && east_west_empty {
        state = state.set(props::SOUTH_REDSTONE, RedstoneSide::Side);
    }
    state
}

impl BlockBehaviour for RedstoneWireBlock {
    fn get_state_for_placement(
        &self,
        _block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(get_connection_state(
            &*level,
            cross_state(),
            context.get_clicked_pos(),
        ))
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = pos.below();
        can_survive_on(level, below, level.get_block_state(below))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction == Direction::Down {
                if !can_survive_on(&*level, neighbour_pos, neighbour_state) {
                    air()
                } else {
                    state
                }
            } else if direction == Direction::Up {
                get_connection_state(&*level, state, pos)
            } else {
                let can_connect_up = !level.get_block_state(pos.above()).is_redstone_conductor();
                let side_connection = get_connecting_side(&*level, pos, direction, can_connect_up);
                if is_connected(side_connection)
                    == is_connected(state.get(side_property(direction)))
                    && !is_cross(state)
                {
                    state.set(side_property(direction), side_connection)
                } else {
                    get_connection_state(
                        &*level,
                        cross_state()
                            .set(props::POWER, state.get(props::POWER))
                            .set(side_property(direction), side_connection),
                        pos,
                    )
                }
            }
        }
    );
    fn update_indirect_neighbour_shapes(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        update_flags: u32,
        update_limit: i32,
    ) {
        for direction in Direction::HORIZONTAL {
            let value = state.get(side_property(direction));
            if value != RedstoneSide::None
                && !level
                    .get_block_state(pos.relative(direction))
                    .is_block(state.block())
            {
                let down_pos = pos.relative(direction).below();
                if level.get_block_state(down_pos).is_block(state.block()) {
                    let neighbor_pos = down_pos.relative(direction.get_opposite());
                    let neighbor_state = level.get_block_state(neighbor_pos);
                    level.neighbor_shape_changed(
                        direction.get_opposite(),
                        down_pos,
                        neighbor_pos,
                        neighbor_state,
                        update_flags,
                        update_limit,
                    );
                }
                let up_pos = pos.relative(direction).above();
                if level.get_block_state(up_pos).is_block(state.block()) {
                    let neighbor_pos = up_pos.relative(direction.get_opposite());
                    let neighbor_state = level.get_block_state(neighbor_pos);
                    level.neighbor_shape_changed(
                        direction.get_opposite(),
                        up_pos,
                        neighbor_pos,
                        neighbor_state,
                        update_flags,
                        update_limit,
                    );
                }
            }
        }
    }
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        if !old_state.is_block(state.block()) {
            update_power_strength(level, pos, state, None, true);
            for direction in Direction::VERTICAL {
                level.update_neighbors_at(pos.relative(direction), Block::REDSTONE_WIRE);
            }
            update_neighbors_of_neighboring_wires(level, pos);
        }
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        moved_by_piston: bool,
    ) {
        if !moved_by_piston {
            for direction in Direction::ALL {
                level.update_neighbors_at(pos.relative(direction), Block::REDSTONE_WIRE);
            }
            update_power_strength(level, pos, state, None, false);
            update_neighbors_of_neighboring_wires(level, pos);
        }
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        if block != Block::REDSTONE_WIRE || !use_experimental_evaluator(level) {
            if state.can_survive(&*level, pos) {
                update_power_strength(level, pos, state, orientation, false);
            } else {
                helpers::drop_resources(level, state, pos, None);
                level.remove_block(pos, false);
            }
        }
    }
    fn get_direct_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if !should_signal() {
            0
        } else {
            state.get_signal(level, pos, direction)
        }
    }
    fn get_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if should_signal() && direction != Direction::Down {
            let power = self.own_signal(state, level, pos);
            if power == 0 {
                0
            } else if direction != Direction::Up
                && !is_connected(
                    get_connection_state(level, state, pos)
                        .get(side_property(direction.get_opposite())),
                )
            {
                0
            } else {
                power
            }
        } else {
            0
        }
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut crate::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> InteractionResult {
        if !player.player.abilities.may_build {
            return InteractionResult::PASS;
        }
        if is_cross(state) || is_dot(state) {
            let mut new_state = if is_cross(state) {
                Block::REDSTONE_WIRE.default_state()
            } else {
                cross_state()
            };
            new_state = new_state.set(props::POWER, state.get(props::POWER));
            new_state = get_connection_state(&*level, new_state, pos);
            if new_state != state {
                level.set_block_and_update(pos, new_state);
                updates_on_shape_change(level, pos, state, new_state);
                return InteractionResult::SUCCESS;
            }
        }
        InteractionResult::PASS
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        state.get(props::POWER)
    }
    fn should_redstone_wire_connect_to(
        &self,
        _state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        _direction: Option<Direction>,
    ) -> bool {
        true
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_sides(
            state,
            rotation,
            [
                props::NORTH_REDSTONE,
                props::EAST_REDSTONE,
                props::SOUTH_REDSTONE,
                props::WEST_REDSTONE,
            ],
        )
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_sides(
            state,
            mirror,
            [
                props::NORTH_REDSTONE,
                props::EAST_REDSTONE,
                props::SOUTH_REDSTONE,
                props::WEST_REDSTONE,
            ],
        )
    }
}

register_block_class!("RedstoneWireBlock", |_| Box::new(RedstoneWireBlock));
