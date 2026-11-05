//! `net.minecraft.world.level.SignalGetter` (default methods of the level interface): redstone signal queries.
//!
//! Free functions take `&dyn BlockGetter` (what block behaviours receive); [`SignalGetter`] offers the same names as
//! methods on every sized `BlockGetter` (e.g. `ServerLevel`: `level.has_neighbor_signal(pos)`).

use super::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::diode_block::is_diode;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::props;

/// `SignalGetter.getDirectSignal(pos, direction)`.
pub fn get_direct_signal(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
    level.get_block_state(pos).get_direct_signal(level, pos, direction)
}

/// `SignalGetter.getDirectSignalTo(pos)`.
pub fn get_direct_signal_to(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    let mut result = 0;
    for (neighbour, direction) in [
        (pos.below(), Direction::Down),
        (pos.above(), Direction::Up),
        (pos.north(), Direction::North),
        (pos.south(), Direction::South),
        (pos.west(), Direction::West),
        (pos.east(), Direction::East),
    ] {
        result = result.max(get_direct_signal(level, neighbour, direction));
        if result >= 15 {
            return result;
        }
    }
    result
}

/// `SignalGetter.getControlInputSignal(pos, direction, onlyDiodes)`.
pub fn get_control_input_signal(level: &dyn BlockGetter, pos: BlockPos, direction: Direction, only_diodes: bool) -> i32 {
    let state = level.get_block_state(pos);
    if only_diodes {
        if is_diode(state) { get_direct_signal(level, pos, direction) } else { 0 }
    } else if state.is_block(Block::REDSTONE_BLOCK) {
        15
    } else if state.is_block(Block::REDSTONE_WIRE) {
        state.get(props::POWER)
    } else if state.is_signal_source() {
        get_direct_signal(level, pos, direction)
    } else {
        0
    }
}

/// `SignalGetter.hasSignal(pos, direction)`.
pub fn has_signal(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> bool {
    get_signal(level, pos, direction) > 0
}

/// `SignalGetter.getSignal(pos, direction)`.
pub fn get_signal(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
    let state = level.get_block_state(pos);
    let signal = state.get_signal(level, pos, direction);
    if state.is_redstone_conductor() { signal.max(get_direct_signal_to(level, pos)) } else { signal }
}

/// `SignalGetter.getBestOwnOrNeighbourSignal(pos)`.
pub fn get_best_own_or_neighbour_signal(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    let state = level.get_block_state(pos);
    get_best_neighbor_signal(level, pos).max(if state.is_signal_source() { state.get_own_signal(level, pos) } else { 0 })
}

/// `SignalGetter.hasNeighborSignal(pos)`.
pub fn has_neighbor_signal(level: &dyn BlockGetter, pos: BlockPos) -> bool {
    get_signal(level, pos.below(), Direction::Down) > 0
        || get_signal(level, pos.above(), Direction::Up) > 0
        || get_signal(level, pos.north(), Direction::North) > 0
        || get_signal(level, pos.south(), Direction::South) > 0
        || get_signal(level, pos.west(), Direction::West) > 0
        || get_signal(level, pos.east(), Direction::East) > 0
}

/// `SignalGetter.getBestNeighborSignal(pos)` (direction order = `Direction.values()`).
pub fn get_best_neighbor_signal(level: &dyn BlockGetter, pos: BlockPos) -> i32 {
    let mut best = 0;
    for direction in Direction::ALL {
        let signal = get_signal(level, pos.relative(direction), direction);
        if signal >= 15 {
            return 15;
        }
        best = best.max(signal);
    }
    best
}

/// The `SignalGetter` default methods on sized level types.
pub trait SignalGetter: BlockGetter + Sized {
    fn get_direct_signal(&self, pos: BlockPos, direction: Direction) -> i32 {
        get_direct_signal(self, pos, direction)
    }
    fn get_direct_signal_to(&self, pos: BlockPos) -> i32 {
        get_direct_signal_to(self, pos)
    }
    fn get_control_input_signal(&self, pos: BlockPos, direction: Direction, only_diodes: bool) -> i32 {
        get_control_input_signal(self, pos, direction, only_diodes)
    }
    fn has_signal(&self, pos: BlockPos, direction: Direction) -> bool {
        has_signal(self, pos, direction)
    }
    fn get_signal(&self, pos: BlockPos, direction: Direction) -> i32 {
        get_signal(self, pos, direction)
    }
    fn get_best_own_or_neighbour_signal(&self, pos: BlockPos) -> i32 {
        get_best_own_or_neighbour_signal(self, pos)
    }
    fn has_neighbor_signal(&self, pos: BlockPos) -> bool {
        has_neighbor_signal(self, pos)
    }
    fn get_best_neighbor_signal(&self, pos: BlockPos) -> i32 {
        get_best_neighbor_signal(self, pos)
    }
}

impl<T: BlockGetter> SignalGetter for T {}
