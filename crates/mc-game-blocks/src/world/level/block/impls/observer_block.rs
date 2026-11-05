//! `net.minecraft.world.level.block.ObserverBlock` (extends `DirectionalBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::server::level::ServerLevel;

pub struct ObserverBlock;

/// `ObserverBlock.updateNeighborsInFront`.
fn update_neighbors_in_front(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let direction = state.get(props::FACING);
    let opposite_pos = pos.relative(direction.get_opposite());
    let orientation = level.initial_orientation(Some(direction.get_opposite()), None);
    level.neighbor_changed(opposite_pos, state.block(), orientation);
    level.update_neighbors_at_except_from_facing(opposite_pos, state.block(), direction, orientation);
}

impl BlockBehaviour for ObserverBlock {
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::POWERED) {
            level.set_block_flags(pos, state.set(props::POWERED, false), 2);
        } else {
            level.set_block_flags(pos, state.set(props::POWERED, true), 2);
            level.schedule_tick_block(pos, state.block(), 2);
        }
        update_neighbors_in_front(level, pos, state);
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        // `startSignal`: `!level.isClientSide()` always holds on the server
        if state.get(props::FACING) == direction && !state.get(props::POWERED) && !level.has_scheduled_block_tick(pos, state.block()) {
            helpers::schedule_block_tick(level, pos, state.block(), 2);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
    fn should_redstone_wire_connect_to(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Option<Direction>) -> bool {
        direction == Some(state.get(props::FACING))
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
    fn get_direct_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        state.get_signal(level, pos, direction)
    }
    fn get_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        if state.get(props::FACING) == direction { self.own_signal(state, level, pos) } else { 0 }
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if !state.is_block(old_state.block()) && state.get(props::POWERED) && !level.has_scheduled_block_tick(pos, state.block()) {
            let new_state = state.set(props::POWERED, false);
            level.set_block_flags(pos, new_state, 18);
            update_neighbors_in_front(level, pos, new_state);
        }
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        if state.get(props::POWERED) && level.has_scheduled_block_tick(pos, state.block()) {
            update_neighbors_in_front(level, pos, state.set(props::POWERED, false));
        }
    }
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &mc_game_core::world::item::BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::FACING, context.get_nearest_looking_direction()))
    }
}

register_block_class!("ObserverBlock", |_| Box::new(ObserverBlock));
