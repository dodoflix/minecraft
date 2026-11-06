//! `net.minecraft.world.level.block.RedstoneTorchBlock` (extends `BaseTorchBlock`).
use super::base_torch_block::BaseTorchBlock;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::redstone::{ExperimentalRedstoneUtils, Orientation};
use mc_game_core::world::level::signal_getter::has_signal;

pub struct RedstoneTorchBlock;

pub const RECENT_TOGGLE_TIMER: i64 = 60;
pub const MAX_RECENT_TOGGLES: usize = 8;
pub const RESTART_DELAY: i32 = 160;
const TOGGLE_DELAY: i32 = 2;

/// `notifyNeighbors(level, pos, state)`; `orientation` = `randomOrientation(level, state)`.
fn notify_neighbors(
    level: &mut ServerLevel,
    pos: BlockPos,
    block: Block,
    orientation: Option<Orientation>,
) {
    for direction in Direction::ALL {
        level.update_neighbors_at_with_orientation(
            pos.relative(direction),
            block,
            ExperimentalRedstoneUtils::with_front(orientation, direction),
        );
    }
}

/// `isToggledTooFrequently(level, pos, add)`.
fn is_toggled_too_frequently(level: &mut ServerLevel, pos: BlockPos, add: bool) -> bool {
    let game_time = level.game_time();
    let toggles = &mut level.tick_state.recent_redstone_torch_toggles;
    if add {
        toggles.push((pos, game_time));
    }
    toggles
        .iter()
        .filter(|(toggle_pos, _)| *toggle_pos == pos)
        .count()
        >= MAX_RECENT_TOGGLES
}

/// `RedstoneTorchBlock.tick`, with `hasNeighborSignal` supplied by the torch variant.
pub fn tick(state: BlockState, level: &mut ServerLevel, pos: BlockPos, neighbor_signal: bool) {
    let game_time = level.game_time();
    let toggles = &mut level.tick_state.recent_redstone_torch_toggles;
    let expired = toggles
        .iter()
        .take_while(|(_, when)| game_time - when > RECENT_TOGGLE_TIMER)
        .count();
    toggles.drain(..expired);
    if state.get(props::LIT) {
        if neighbor_signal {
            level.set_block_and_update(pos, state.set(props::LIT, false));
            if is_toggled_too_frequently(level, pos, true) {
                level.level_event(None, 1502, pos, 0);
                let block = BlockGetter::get_block_state(level, pos).block();
                level.schedule_tick_block(pos, block, RESTART_DELAY);
            }
        }
    } else if !neighbor_signal && !is_toggled_too_frequently(level, pos, false) {
        level.set_block_and_update(pos, state.set(props::LIT, true));
    }
}

/// `RedstoneTorchBlock.neighborChanged`.
pub fn neighbor_changed(
    state: BlockState,
    level: &mut ServerLevel,
    pos: BlockPos,
    neighbor_signal: bool,
) {
    if state.get(props::LIT) == neighbor_signal
        && !level.will_tick_block_this_tick(pos, state.block())
    {
        level.schedule_tick_block(pos, state.block(), TOGGLE_DELAY);
    }
}

impl BlockBehaviour for RedstoneTorchBlock {
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        direction_to_neighbour: Direction,
        neighbour_pos: BlockPos,
        neighbour_state: BlockState,
    ) -> BlockState {
        BaseTorchBlock.update_shape(
            state,
            level,
            pos,
            direction_to_neighbour,
            neighbour_pos,
            neighbour_state,
        )
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        BaseTorchBlock.can_survive(state, level, pos)
    }
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        let orientation =
            ExperimentalRedstoneUtils::initial_orientation(level, None, Some(Direction::Up));
        notify_neighbors(level, pos, state.block(), orientation);
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        moved_by_piston: bool,
    ) {
        if !moved_by_piston {
            let orientation =
                ExperimentalRedstoneUtils::initial_orientation(level, None, Some(Direction::Up));
            notify_neighbors(level, pos, state.block(), orientation);
        }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let neighbor_signal = has_signal(level, pos.below(), Direction::Down);
        tick(state, level, pos, neighbor_signal);
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        let neighbor_signal = has_signal(level, pos.below(), Direction::Down);
        neighbor_changed(state, level, pos, neighbor_signal);
    }
    fn get_direct_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if direction == Direction::Down {
            state.get_signal(level, pos, direction)
        } else {
            0
        }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::LIT) { 15 } else { 0 }
    }
    fn get_signal(
        &self,
        state: BlockState,
        level: &dyn BlockGetter,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if Direction::Up != direction {
            self.own_signal(state, level, pos)
        } else {
            0
        }
    }
}

/// `RedstoneTorchBlock.notifyNeighbors` for the wall variant (`randomOrientation` differs).
pub fn notify_neighbors_wall(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let orientation = ExperimentalRedstoneUtils::initial_orientation(
        level,
        Some(state.get(props::HORIZONTAL_FACING).get_opposite()),
        Some(Direction::Up),
    );
    notify_neighbors(level, pos, state.block(), orientation);
}

register_block_class!("RedstoneTorchBlock", |_| Box::new(RedstoneTorchBlock));
