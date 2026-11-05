//! `net.minecraft.world.level.block.TripWireHookBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::gameevent::GameEventContext;
use mc_data::{GameEvent, SoundEvent};
use mc_protocol::packets::play::level::SoundSource;

pub struct TripWireHookBlock;

/// `TripWireHookBlock.calculateState`.
pub fn calculate_state(
    level: &mut ServerLevel,
    pos: BlockPos,
    state: BlockState,
    is_being_destroyed: bool,
    can_update: bool,
    wire_source: i32,
    wire_source_state: Option<BlockState>,
) {
    let Some(direction) = state.try_get(props::HORIZONTAL_FACING) else { return };
    let was_attached = state.try_get(props::ATTACHED).unwrap_or(false);
    let was_powered = state.try_get(props::POWERED).unwrap_or(false);
    let block = state.block();
    let mut attached = !is_being_destroyed;
    let mut powered = false;
    let mut receiver_pos = 0;
    let mut wire_states: [Option<BlockState>; 42] = [None; 42];
    for i in 1..42 {
        let test_pos = pos.relative_n(direction, i);
        let mut wire_state = level.get_block_state(test_pos);
        if wire_state.is_block(Block::TRIPWIRE_HOOK) {
            if wire_state.get(props::HORIZONTAL_FACING) == direction.get_opposite() {
                receiver_pos = i;
            }
            break;
        }
        if !wire_state.is_block(Block::TRIPWIRE) && i != wire_source {
            wire_states[i as usize] = None;
            attached = false;
        } else {
            if i == wire_source {
                wire_state = wire_source_state.unwrap_or(wire_state);
            }
            let wire_armed = !wire_state.get(props::DISARMED);
            let wire_powered = wire_state.get(props::POWERED);
            powered |= wire_armed && wire_powered;
            wire_states[i as usize] = Some(wire_state);
            if i == wire_source {
                level.schedule_tick_block(pos, block, 10);
                attached &= wire_armed;
            }
        }
    }
    attached &= receiver_pos > 1;
    powered &= attached;
    let new_state = block.default_state().set(props::ATTACHED, attached).set(props::POWERED, powered);
    if receiver_pos > 0 {
        let test_pos = pos.relative_n(direction, receiver_pos);
        let opposite = direction.get_opposite();
        level.set_block_and_update(test_pos, new_state.set(props::HORIZONTAL_FACING, opposite));
        notify_neighbors(block, level, test_pos, opposite);
        if !level.get_block_state(pos).is_block(Block::TRIPWIRE_HOOK) {
            on_removed(new_state, level, pos);
            return;
        }
        emit_state(level, test_pos, attached, powered, was_attached, was_powered);
    }
    emit_state(level, pos, attached, powered, was_attached, was_powered);
    if !is_being_destroyed {
        level.set_block_and_update(pos, new_state.set(props::HORIZONTAL_FACING, direction));
        if can_update {
            notify_neighbors(block, level, pos, direction);
        }
    }
    if was_attached != attached {
        for i in 1..receiver_pos {
            let test_pos = pos.relative_n(direction, i);
            if let Some(wire_data) = wire_states[i as usize] {
                let test_pos_state = level.get_block_state(test_pos);
                if test_pos_state.is_block(Block::TRIPWIRE) || test_pos_state.is_block(Block::TRIPWIRE_HOOK) {
                    level.set_block_and_update(test_pos, wire_data.try_set(props::ATTACHED, attached).unwrap_or(wire_data));
                }
            }
        }
    }
}

/// `TripWireHookBlock.emitState`.
fn emit_state(level: &mut ServerLevel, pos: BlockPos, attached: bool, powered: bool, was_attached: bool, was_powered: bool) {
    if powered && !was_powered {
        level.play_sound(None, pos, SoundEvent::BLOCK_TRIPWIRE_CLICK_ON, SoundSource::Blocks, 0.4, 0.6);
        level.game_event_at_block(GameEvent::BLOCK_ACTIVATE, pos, &GameEventContext::of_entity(None));
    } else if !powered && was_powered {
        level.play_sound(None, pos, SoundEvent::BLOCK_TRIPWIRE_CLICK_OFF, SoundSource::Blocks, 0.4, 0.5);
        level.game_event_at_block(GameEvent::BLOCK_DEACTIVATE, pos, &GameEventContext::of_entity(None));
    } else if attached && !was_attached {
        level.play_sound(None, pos, SoundEvent::BLOCK_TRIPWIRE_ATTACH, SoundSource::Blocks, 0.4, 0.7);
        level.game_event_at_block(GameEvent::BLOCK_ATTACH, pos, &GameEventContext::of_entity(None));
    } else if !attached && was_attached {
        let pitch = 1.2 / (level.random().next_float() * 0.2 + 0.9);
        level.play_sound(None, pos, SoundEvent::BLOCK_TRIPWIRE_DETACH, SoundSource::Blocks, 0.4, pitch);
        level.game_event_at_block(GameEvent::BLOCK_DETACH, pos, &GameEventContext::of_entity(None));
    }
}

/// `TripWireHookBlock.notifyNeighbors`.
fn notify_neighbors(block: Block, level: &mut ServerLevel, pos: BlockPos, direction: Direction) {
    let front = direction.get_opposite();
    let orientation = level.initial_orientation(Some(front), Some(Direction::Up));
    level.update_neighbors_at_with_orientation(pos, block, orientation);
    level.update_neighbors_at_with_orientation(pos.relative(front), block, orientation);
}

/// `TripWireHookBlock.onRemoved`.
fn on_removed(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let attached = state.get(props::ATTACHED);
    let powered = state.get(props::POWERED);
    if attached || powered {
        calculate_state(level, pos, state, true, false, -1, None);
    }
    if powered {
        notify_neighbors(state.block(), level, pos, state.get(props::HORIZONTAL_FACING));
    }
}

impl BlockBehaviour for TripWireHookBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let direction = state.get(props::HORIZONTAL_FACING);
        let relative = pos.relative(direction.get_opposite());
        let block_state = level.get_block_state(relative);
        direction.get_axis().is_horizontal() && block_state.is_face_sturdy_at(level, relative, direction)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) { air() } else { state }
    });
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let mut state = block.default_state().set(props::POWERED, false).set(props::ATTACHED, false);
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis().is_horizontal() {
                state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
                if state.can_survive(&*level, context.get_clicked_pos()) {
                    return Some(state);
                }
            }
        }
        None
    }
    fn set_placed_by(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _placer: Option<&mut crate::world::entity::ServerPlayer>, _stack: &crate::world::item::ItemStack) {
        calculate_state(level, pos, state, false, false, -1, None);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        calculate_state(level, pos, state, false, true, -1, None);
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) {
        if !moved_by_piston {
            on_removed(state, level, pos);
        }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
    fn get_direct_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Direction) -> i32 {
        if !state.get(props::POWERED) {
            0
        } else if state.get(props::HORIZONTAL_FACING) == direction {
            15
        } else {
            0
        }
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("TripWireHookBlock", |_| Box::new(TripWireHookBlock));
