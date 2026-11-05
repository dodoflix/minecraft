//! `net.minecraft.world.level.block.LeverBlock` (extends `FaceAttachedHorizontalDirectionalBlock`).
use super::face_attached_horizontal_directional_block::{self as face_attached, get_connected_direction};
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity as _, EntityId, ServerPlayer};
use crate::world::item::InteractionResult;
use crate::world::level::gameevent::GameEventContext;
use mc_data::GameEvent;
use mc_protocol::packets::play::level::SoundSource;

pub struct LeverBlock;

/// The `updateNeighbours` shared by `LeverBlock` and `ButtonBlock`.
pub fn update_neighbours(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    let front = get_connected_direction(state).get_opposite();
    let up = if front.get_axis().is_horizontal() { Direction::Up } else { state.get(props::HORIZONTAL_FACING) };
    let orientation = level.initial_orientation(Some(front), Some(up));
    level.update_neighbors_at_with_orientation(pos, state.block(), orientation);
    level.update_neighbors_at_with_orientation(pos.relative(front), state.block(), orientation);
}

/// `LeverBlock.pull(state, level, pos, player)`.
pub fn pull(state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: Option<EntityId>) {
    let state = state.cycle(props::POWERED);
    level.set_block_and_update(pos, state);
    update_neighbours(state, level, pos);
    let pitch = if state.get(props::POWERED) { 0.6 } else { 0.5 };
    level.play_sound(player, pos, mc_data::SoundEvent::BLOCK_LEVER_CLICK, SoundSource::Blocks, 0.3, pitch);
    level.game_event_at_block(if state.get(props::POWERED) { GameEvent::BLOCK_ACTIVATE } else { GameEvent::BLOCK_DEACTIVATE }, pos, &GameEventContext::of_entity(player));
}

impl BlockBehaviour for LeverBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        face_attached::can_survive(state, level, pos)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        face_attached::get_state_for_placement(block, &*level, context)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        face_attached::update_shape(state, &*level, pos, direction)
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        // `pull(stateBefore, level, pos, null)`: the player is not passed on (sound and game event have no source)
        pull(state, level, pos, None);
        InteractionResult::SUCCESS
    }
    fn on_explosion_hit(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        explosion: &dyn crate::world::level::explosion::Explosion,
        on_hit: &mut dyn FnMut(crate::world::item::ItemStack, BlockPos),
    ) {
        if explosion.can_trigger_blocks() {
            pull(state, level, pos, None);
        }
        crate::world::level::explosion::default_on_explosion_hit(state, level, pos, explosion, on_hit);
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) {
        if !moved_by_piston && state.get(props::POWERED) {
            update_neighbours(state, level, pos);
        }
    }
    fn get_direct_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Direction) -> i32 {
        if state.get(props::POWERED) && get_connected_direction(state) == direction { 15 } else { 0 }
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
}

register_block_class!("LeverBlock", |_| Box::new(LeverBlock));
