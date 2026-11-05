//! `net.minecraft.world.level.block.ButtonBlock` (extends `FaceAttachedHorizontalDirectionalBlock`; all wooden/stone
//! buttons, parameterized by `BlockSetType` and `ticksToStayPressed`).
use super::face_attached_horizontal_directional_block::{self as face_attached, get_connected_direction};
use super::lever_block::update_neighbours;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_set_type::BlockSetType;
use crate::world::entity::{Entity as _, EntityId, ServerPlayer};
use crate::world::item::InteractionResult;
use crate::world::level::gameevent::GameEventContext;
use mc_data::GameEvent;
use mc_protocol::packets::play::level::SoundSource;

pub struct ButtonBlock {
    ty: &'static BlockSetType,
    ticks_to_stay_pressed: i32,
}

impl ButtonBlock {
    /// `new ButtonBlock(type, ticksToStayPressed, ...)` as constructed by `Blocks` (`stone`/`polished_blackstone` 20, else 30).
    pub fn new(block: Block) -> Self {
        let ty = BlockSetType::for_block(block).expect("every button block has a block set type");
        Self { ty, ticks_to_stay_pressed: if ty.name == "stone" { 20 } else { 30 } }
    }

    /// `ButtonBlock.press(state, level, pos, player)`.
    pub fn press(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: Option<EntityId>) {
        level.set_block_and_update(pos, state.set(props::POWERED, true));
        update_neighbours(state, level, pos);
        level.schedule_tick_block(pos, state.block(), self.ticks_to_stay_pressed);
        self.play_sound(player, level, pos, true);
        level.game_event_at_block(GameEvent::BLOCK_ACTIVATE, pos, &GameEventContext::of_entity(player));
    }

    /// `ButtonBlock.playSound(player, level, pos, pressed)`.
    fn play_sound(&self, player: Option<EntityId>, level: &mut ServerLevel, pos: BlockPos, pressed: bool) {
        let sound = if pressed { self.ty.button_click_on } else { self.ty.button_click_off };
        level.play_sound(if pressed { player } else { None }, pos, sound, SoundSource::Blocks, 1.0, 1.0);
    }

    /// `ButtonBlock.checkPressed(state, level, pos)`.
    pub fn check_pressed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let first_arrow = if self.ty.can_button_be_activated_by_arrows {
            let bounds = state.get_shape(&*level, pos).bounds().move_(pos.x() as f64, pos.y() as f64, pos.z() as f64);
            level.get_entities_of_class(&bounds, |e| e.is_abstract_arrow(), |_| true).into_iter().next()
        } else {
            None
        };
        let should_be_pressed = first_arrow.is_some();
        let was_pressed = state.get(props::POWERED);
        if should_be_pressed != was_pressed {
            level.set_block_and_update(pos, state.set(props::POWERED, should_be_pressed));
            update_neighbours(state, level, pos);
            self.play_sound(None, level, pos, should_be_pressed);
            level.game_event_at_block(if should_be_pressed { GameEvent::BLOCK_ACTIVATE } else { GameEvent::BLOCK_DEACTIVATE }, pos, &GameEventContext::of_entity(first_arrow));
        }
        if should_be_pressed {
            level.schedule_tick_block(pos, state.block(), self.ticks_to_stay_pressed);
        }
    }
}

impl BlockBehaviour for ButtonBlock {
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
    fn use_without_item(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> InteractionResult {
        if state.get(props::POWERED) {
            InteractionResult::CONSUME
        } else {
            self.press(state, level, pos, Some(player.id()));
            InteractionResult::SUCCESS
        }
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
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if state.get(props::POWERED) {
            self.check_pressed(state, level, pos);
        }
    }
}

register_block_class!("ButtonBlock", |block| Box::new(ButtonBlock::new(block)));
