//! `net.minecraft.world.level.redstone.ExperimentalRedstoneWireEvaluator`.
use super::redstone_wire_evaluator::RedstoneWireEvaluator;
use super::{Orientation, SideBias};
use crate::server::level::ServerLevel;
use crate::world::level::block::BlockStateExt;
use crate::world::level::{BlockGetter, LevelAccessor};
use indexmap::IndexMap;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::enums::RedstoneSide;
use mc_data::props;
use std::collections::VecDeque;

#[derive(Default)]
pub struct ExperimentalRedstoneWireEvaluator {
    wires_to_turn_off: VecDeque<BlockPos>,
    wires_to_turn_on: VecDeque<BlockPos>,
    /// `Object2IntLinkedOpenHashMap<BlockPos>`: insertion order, value = `orientation index << 4 | power`.
    updated_wires: IndexMap<BlockPos, i32>,
}

fn pack_orientation_and_power(orientation: Orientation, power: i32) -> i32 {
    (orientation.get_index() as i32) << 4 | power
}

fn unpack_orientation(packed: i32) -> Orientation {
    Orientation::from_index((packed >> 4) as usize)
}

fn unpack_power(packed: i32) -> i32 {
    packed & 15
}

fn is_connected(state: BlockState, direction: Direction) -> bool {
    match direction {
        Direction::North => state.get(props::NORTH_REDSTONE) != RedstoneSide::None,
        Direction::East => state.get(props::EAST_REDSTONE) != RedstoneSide::None,
        Direction::South => state.get(props::SOUTH_REDSTONE) != RedstoneSide::None,
        Direction::West => state.get(props::WEST_REDSTONE) != RedstoneSide::None,
        _ => direction == Direction::Down,
    }
}

fn get_initial_orientation(level: &mut ServerLevel, incoming: Option<Orientation>) -> Orientation {
    let orientation = incoming.unwrap_or_else(|| Orientation::random(LevelAccessor::random(level)));
    orientation
        .with_up(Direction::Up)
        .with_side_bias(SideBias::Left)
}

impl ExperimentalRedstoneWireEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    fn cause_neighbor_updates(&self, level: &mut ServerLevel) {
        for (&wire_pos, &packed) in &self.updated_wires {
            let orientation = unpack_orientation(packed);
            let state = BlockGetter::get_block_state(level, wire_pos);
            for neighbor_direction in orientation.get_directions() {
                if is_connected(state, neighbor_direction) {
                    let neighbor_pos = wire_pos.relative(neighbor_direction);
                    let neighbor_state = BlockGetter::get_block_state(level, neighbor_pos);
                    let neighbor_orientation =
                        orientation.with_front_preserve_up(neighbor_direction);
                    level.neighbor_changed_with_state(
                        neighbor_state,
                        neighbor_pos,
                        Block::REDSTONE_WIRE,
                        Some(neighbor_orientation),
                        false,
                    );
                    if neighbor_state.is_redstone_conductor() {
                        for direction in neighbor_orientation.get_directions() {
                            if direction != neighbor_direction.get_opposite() {
                                level.neighbor_changed(
                                    neighbor_pos.relative(direction),
                                    Block::REDSTONE_WIRE,
                                    Some(neighbor_orientation.with_front_preserve_up(direction)),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn calculate_current_changes(
        &mut self,
        level: &mut ServerLevel,
        initial_position: BlockPos,
        initial_orientation: Orientation,
    ) {
        let initial_state = BlockGetter::get_block_state(level, initial_position);
        if initial_state.is_block(Block::REDSTONE_WIRE) {
            self.set_power(
                initial_position,
                initial_state.get(props::POWER),
                initial_orientation,
            );
            self.wires_to_turn_off.push_back(initial_position);
        } else {
            self.propagate_change_to_neighbors(
                level,
                initial_position,
                0,
                initial_orientation,
                true,
            );
        }

        while let Some(pos) = self.wires_to_turn_off.pop_front() {
            let packed = self.updated_wires[&pos];
            let orientation = unpack_orientation(packed);
            let old_power = unpack_power(packed);
            let block_power = self.get_block_signal(level, pos);
            let wire_power = self.get_incoming_wire_signal(level, pos);
            let new_power = block_power.max(wire_power);
            let power_to_set;
            if new_power < old_power {
                if block_power > 0 && !self.wires_to_turn_on.contains(&pos) {
                    self.wires_to_turn_on.push_back(pos);
                }
                power_to_set = 0;
            } else {
                power_to_set = new_power;
            }
            if power_to_set != old_power {
                self.set_power(pos, power_to_set, orientation);
            }
            self.propagate_change_to_neighbors(
                level,
                pos,
                power_to_set,
                orientation,
                old_power > new_power,
            );
        }

        while let Some(pos) = self.wires_to_turn_on.pop_front() {
            let packed = self.updated_wires[&pos];
            let old_power = unpack_power(packed);
            let block_power = self.get_block_signal(level, pos);
            let wire_power = self.get_incoming_wire_signal(level, pos);
            let new_power = block_power.max(wire_power);
            let orientation = unpack_orientation(packed);
            if new_power > old_power {
                self.set_power(pos, new_power, orientation);
            } else if new_power < old_power {
                panic!("Turning off wire while trying to turn it on. Should not happen.");
            }
            self.propagate_change_to_neighbors(level, pos, new_power, orientation, false);
        }
    }

    fn set_power(&mut self, pos: BlockPos, new_power: i32, orientation: Orientation) {
        let packed = match self.updated_wires.get(&pos) {
            None => pack_orientation_and_power(orientation, new_power),
            Some(&packed) => pack_orientation_and_power(unpack_orientation(packed), new_power),
        };
        self.updated_wires.insert(pos, packed);
    }

    fn propagate_change_to_neighbors(
        &mut self,
        level: &mut ServerLevel,
        pos: BlockPos,
        new_power: i32,
        orientation: Orientation,
        allow_turning_off: bool,
    ) {
        let horizontal = orientation.get_horizontal_directions();
        for &direction_horizontal in &horizontal {
            let offset_pos = pos.relative(direction_horizontal);
            self.enqueue_neighbor_wire(
                level,
                offset_pos,
                new_power,
                orientation.with_front(direction_horizontal),
                allow_turning_off,
            );
        }
        for direction_vertical in orientation.get_vertical_directions() {
            let offset_pos = pos.relative(direction_vertical);
            let solid_block =
                BlockGetter::get_block_state(level, offset_pos).is_redstone_conductor();
            for &direction_horizontal in &horizontal {
                let neighbor = pos.relative(direction_horizontal);
                if direction_vertical == Direction::Up && !solid_block {
                    let neighbor_wire = offset_pos.relative(direction_horizontal);
                    self.enqueue_neighbor_wire(
                        level,
                        neighbor_wire,
                        new_power,
                        orientation.with_front(direction_horizontal),
                        allow_turning_off,
                    );
                } else if direction_vertical == Direction::Down
                    && !BlockGetter::get_block_state(level, neighbor).is_redstone_conductor()
                {
                    let neighbor_wire = offset_pos.relative(direction_horizontal);
                    self.enqueue_neighbor_wire(
                        level,
                        neighbor_wire,
                        new_power,
                        orientation.with_front(direction_horizontal),
                        allow_turning_off,
                    );
                }
            }
        }
    }

    fn enqueue_neighbor_wire(
        &mut self,
        level: &ServerLevel,
        pos: BlockPos,
        new_from_power: i32,
        orientation: Orientation,
        allow_turning_off: bool,
    ) {
        let state = BlockGetter::get_block_state(level, pos);
        if state.is_block(Block::REDSTONE_WIRE) {
            let to_power = self.get_wire_signal(pos, state);
            if to_power < new_from_power - 1 && !self.wires_to_turn_on.contains(&pos) {
                self.wires_to_turn_on.push_back(pos);
                self.set_power(pos, to_power, orientation);
            }
            if allow_turning_off
                && to_power > new_from_power
                && !self.wires_to_turn_off.contains(&pos)
            {
                self.wires_to_turn_off.push_back(pos);
                self.set_power(pos, to_power, orientation);
            }
        }
    }
}

impl RedstoneWireEvaluator for ExperimentalRedstoneWireEvaluator {
    fn update_power_strength(
        &mut self,
        level: &mut ServerLevel,
        initial_pos: BlockPos,
        _state: BlockState,
        orientation: Option<Orientation>,
        shape_update_wires_around_initial_position: bool,
    ) {
        let initial_orientation = get_initial_orientation(level, orientation);
        self.calculate_current_changes(level, initial_pos, initial_orientation);
        let mut initial_wire = true;
        let keys: Vec<BlockPos> = self.updated_wires.keys().copied().collect();
        for pos in keys {
            let new_level = unpack_power(self.updated_wires[&pos]);
            let state = BlockGetter::get_block_state(level, pos);
            if state.is_block(Block::REDSTONE_WIRE) && state.get(props::POWER) != new_level {
                let mut update_flags = 2;
                if !shape_update_wires_around_initial_position || !initial_wire {
                    update_flags |= 128;
                }
                level.set_block_flags(pos, state.set(props::POWER, new_level), update_flags);
            } else {
                self.updated_wires.shift_remove(&pos);
            }
            initial_wire = false;
        }
        self.cause_neighbor_updates(level);
    }

    fn get_wire_signal(&self, pos: BlockPos, state: BlockState) -> i32 {
        match self.updated_wires.get(&pos) {
            Some(&packed) => unpack_power(packed),
            None => {
                if state.is_block(Block::REDSTONE_WIRE) {
                    state.get(props::POWER)
                } else {
                    0
                }
            }
        }
    }
}
