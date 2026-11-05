//! `net.minecraft.world.level.block.DiodeBlock` (extends `HorizontalDirectionalBlock`; abstract base of repeater /
//! comparator). The shared logic lives in free functions over [`Diode`] (the overridable part of the Java class);
//! [`diode_behaviour!`] defines the `BlockBehaviour` methods `RepeaterBlock` and `ComparatorBlock` inherit unchanged.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::signal_getter;
use mc_world::ticks::TickPriority;

/// `DiodeBlock.isDiode`.
pub fn is_diode(state: BlockState) -> bool {
    helpers::is_instance_of(state, "DiodeBlock")
}

/// `DiodeBlock.canSurviveOn`.
pub fn can_survive_on(level: &dyn BlockGetter, neighbour_pos: BlockPos, neighbour_state: BlockState) -> bool {
    neighbour_state.is_face_sturdy_support(level, neighbour_pos, Direction::Up, SupportType::Rigid)
}

/// The methods subclasses of `DiodeBlock` override (defaults = `DiodeBlock`).
pub trait Diode {
    /// `getDelay`.
    fn get_delay(&self, state: BlockState) -> i32;
    /// `isLocked`.
    fn is_locked(&self, _level: &dyn BlockGetter, _pos: BlockPos, _state: BlockState) -> bool {
        false
    }
    /// `shouldTurnOn`.
    fn should_turn_on(&self, level: &ServerLevel, pos: BlockPos, state: BlockState) -> bool {
        self.get_input_signal(level, pos, state) > 0
    }
    /// `getInputSignal`.
    fn get_input_signal(&self, level: &ServerLevel, pos: BlockPos, state: BlockState) -> i32 {
        base_input_signal(level, pos, state)
    }
    /// `sideInputDiodesOnly`.
    fn side_input_diodes_only(&self) -> bool {
        false
    }
    /// `getOutputSignal`.
    fn get_output_signal(&self, _level: &dyn BlockGetter, _pos: BlockPos, _state: BlockState) -> i32 {
        15
    }
    /// `tick`.
    fn diode_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        base_tick(self, state, level, pos);
    }
    /// `checkTickOnNeighbor`.
    fn check_tick_on_neighbor(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
        base_check_tick_on_neighbor(self, level, pos, state);
    }
}

/// `DiodeBlock.getInputSignal`.
pub fn base_input_signal(level: &dyn BlockGetter, pos: BlockPos, state: BlockState) -> i32 {
    let direction = state.get(props::HORIZONTAL_FACING);
    let target_pos = pos.relative(direction);
    let input = signal_getter::get_signal(level, target_pos, direction);
    if input >= 15 {
        input
    } else {
        let target_state = level.get_block_state(target_pos);
        input.max(if target_state.is_block(Block::REDSTONE_WIRE) { target_state.get(props::POWER) } else { 0 })
    }
}

/// `DiodeBlock.getAlternateSignal`.
pub fn get_alternate_signal(diode: &(impl Diode + ?Sized), level: &dyn BlockGetter, pos: BlockPos, state: BlockState) -> i32 {
    let direction = state.get(props::HORIZONTAL_FACING);
    let clockwise = direction.get_clockwise();
    let counter_clockwise = direction.get_counter_clockwise();
    let only_diodes = diode.side_input_diodes_only();
    signal_getter::get_control_input_signal(level, pos.relative(clockwise), clockwise, only_diodes)
        .max(signal_getter::get_control_input_signal(level, pos.relative(counter_clockwise), counter_clockwise, only_diodes))
}

/// `DiodeBlock.shouldPrioritize`.
pub fn should_prioritize(level: &dyn BlockGetter, pos: BlockPos, state: BlockState) -> bool {
    let direction = state.get(props::HORIZONTAL_FACING).get_opposite();
    let opposite_state = level.get_block_state(pos.relative(direction));
    is_diode(opposite_state) && opposite_state.get(props::HORIZONTAL_FACING) != direction
}

/// `DiodeBlock.checkTickOnNeighbor`.
pub fn base_check_tick_on_neighbor(diode: &(impl Diode + ?Sized), level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    if !diode.is_locked(&*level, pos, state) {
        let on = state.get(props::POWERED);
        let should_turn_on = diode.should_turn_on(level, pos, state);
        if on != should_turn_on && !level.will_tick_block_this_tick(pos, state.block()) {
            let priority = if should_prioritize(&*level, pos, state) {
                TickPriority::ExtremelyHigh
            } else if on {
                TickPriority::VeryHigh
            } else {
                TickPriority::High
            };
            level.schedule_block_tick(pos, state.block(), diode.get_delay(state), priority);
        }
    }
}

/// `DiodeBlock.tick`.
pub fn base_tick(diode: &(impl Diode + ?Sized), state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if !diode.is_locked(&*level, pos, state) {
        let on = state.get(props::POWERED);
        let should_turn_on = diode.should_turn_on(level, pos, state);
        if on && !should_turn_on {
            level.set_block_flags(pos, state.set(props::POWERED, false), 2);
        } else if !on {
            level.set_block_flags(pos, state.set(props::POWERED, true), 2);
            if !should_turn_on {
                level.schedule_block_tick(pos, state.block(), diode.get_delay(state), TickPriority::VeryHigh);
            }
        }
    }
}

/// `DiodeBlock.neighborChanged`.
pub fn neighbor_changed(diode: &(impl Diode + ?Sized), state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if level.get_block_state(pos).is_block(state.block()) {
        if state.can_survive(&*level, pos) {
            diode.check_tick_on_neighbor(level, pos, state);
        } else {
            helpers::drop_resources(level, state, pos, None);
            level.remove_block(pos, false);
            for direction in Direction::ALL {
                level.update_neighbors_at(pos.relative(direction), state.block());
            }
        }
    }
}

/// `DiodeBlock.setPlacedBy`.
pub fn set_placed_by(diode: &(impl Diode + ?Sized), state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if diode.should_turn_on(level, pos, state) {
        level.schedule_tick_block(pos, state.block(), 1);
    }
}

/// `DiodeBlock.updateNeighborsInFront`.
pub fn update_neighbors_in_front(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let direction = state.get(props::HORIZONTAL_FACING);
    let opposite_pos = pos.relative(direction.get_opposite());
    let orientation = level.initial_orientation(Some(direction.get_opposite()), Some(Direction::Up));
    level.neighbor_changed(opposite_pos, state.block(), orientation);
    level.update_neighbors_at_except_from_facing(opposite_pos, state.block(), direction, orientation);
}

/// `DiodeBlock.getStateForPlacement`.
pub fn get_state_for_placement(block: Block, context: &crate::world::item::BlockPlaceContext) -> BlockState {
    block.default_state().set(props::HORIZONTAL_FACING, context.get_horizontal_direction().get_opposite())
}

/// The `BlockBehaviour` methods defined by `DiodeBlock` itself and inherited unchanged (`canSurvive`, `tick`,
/// `neighborChanged`, `setPlacedBy`, `onPlace`, `affectNeighborsAfterRemoval`, `rotate`, `mirror`, the signal methods).
#[macro_export]
macro_rules! diode_behaviour {
    () => {
        fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
            let below_pos = pos.below();
            $crate::world::level::block::impls::diode_block::can_survive_on(level, below_pos, level.get_block_state(below_pos))
        }
        fn tick(&self, state: BlockState, level: &mut $crate::server::level::ServerLevel, pos: BlockPos) {
            $crate::world::level::block::impls::diode_block::Diode::diode_tick(self, state, level, pos);
        }
        fn neighbor_changed(
            &self,
            state: BlockState,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            _block: Block,
            _orientation: Option<$crate::world::level::redstone::Orientation>,
            _moved_by_piston: bool,
        ) {
            $crate::world::level::block::impls::diode_block::neighbor_changed(self, state, level, pos);
        }
        fn set_placed_by(
            &self,
            state: BlockState,
            level: &mut $crate::server::level::ServerLevel,
            pos: BlockPos,
            _placer: Option<&mut $crate::world::entity::ServerPlayer>,
            _stack: &$crate::world::item::ItemStack,
        ) {
            $crate::world::level::block::impls::diode_block::set_placed_by(self, state, level, pos);
        }
        fn on_place(&self, state: BlockState, level: &mut $crate::server::level::ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
            $crate::world::level::block::impls::diode_block::update_neighbors_in_front(level, pos, state);
        }
        fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut $crate::server::level::ServerLevel, pos: BlockPos, moved_by_piston: bool) {
            if !moved_by_piston {
                $crate::world::level::block::impls::diode_block::update_neighbors_in_front(level, pos, state);
            }
        }
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
        fn own_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> i32 {
            if state.get(props::POWERED) { $crate::world::level::block::impls::diode_block::Diode::get_output_signal(self, level, pos, state) } else { 0 }
        }
    };
}
pub use diode_behaviour;
