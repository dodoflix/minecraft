//! `net.minecraft.world.level.block.RepeaterBlock` (extends `DiodeBlock`).
use super::diode_block::{self, is_diode};
use super::prelude::*;

pub struct RepeaterBlock;

/// `SignalGetter.getControlInputSignal(pos, direction, onlyDiodes = true)`.
fn get_control_input_signal_diodes_only(level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
    let block_state = level.get_block_state(pos);
    if is_diode(block_state) { block_state.get_direct_signal(level, pos, direction) } else { 0 }
}

/// `RepeaterBlock.isLocked` (`getAlternateSignal(level, pos, state) > 0` with `sideInputDiodesOnly`).
fn is_locked(level: &dyn BlockGetter, pos: BlockPos, state: BlockState) -> bool {
    let direction = state.get(props::HORIZONTAL_FACING);
    let clockwise = direction.get_clockwise();
    let counter_clockwise = direction.get_counter_clockwise();
    let alternate = get_control_input_signal_diodes_only(level, pos.relative(clockwise), clockwise)
        .max(get_control_input_signal_diodes_only(level, pos.relative(counter_clockwise), counter_clockwise));
    alternate > 0
}

impl BlockBehaviour for RepeaterBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        diode_block::can_survive_on(level, below_pos, level.get_block_state(below_pos))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !diode_block::can_survive_on(&*level, neighbour_pos, neighbour_state) {
            air()
        } else if direction.get_axis() != state.get(props::HORIZONTAL_FACING).get_axis() {
            // `!level.isClientSide()` is always true for the server levels
            state.set(props::LOCKED, is_locked(&*level, pos, state))
        } else {
            state
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn get_direct_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        state.get_signal(level, pos, direction)
    }
    fn get_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        if state.get(props::HORIZONTAL_FACING) == direction { self.own_signal(state, level, pos) } else { 0 }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
    fn should_redstone_wire_connect_to(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Option<Direction>) -> bool {
        let repeater_direction = state.get(props::HORIZONTAL_FACING);
        Some(repeater_direction) == direction || Some(repeater_direction.get_opposite()) == direction
    }
}

register_block_class!("RepeaterBlock", |_| Box::new(RepeaterBlock));
