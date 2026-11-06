//! `net.minecraft.world.level.block.NoteBlock`.
use mc_core::phys::BlockHitResult;
use mc_data::GameEvent;
use mc_data::enums::NoteBlockInstrument;
use mc_data::misc::NOTE_BLOCK_INSTRUMENTS;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::{Entity as _, EntityId, ServerPlayer};
use mc_game_core::world::item::{BlockPlaceContext, InteractionHand, InteractionResult, ItemStack};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::gameevent::GameEventContext;
use mc_game_core::world::level::redstone::Orientation;
use mc_game_core::world::level::signal_getter::SignalGetter;
use mc_game_core::{register_block_class, update_shape};
use mc_protocol::packets::play::level::SoundSource;

pub struct NoteBlock;

fn works_above_note_block(instrument: NoteBlockInstrument) -> bool {
    NOTE_BLOCK_INSTRUMENTS[instrument as usize].works_above_note_block
}

/// `NoteBlock.setInstrument`.
fn set_instrument(level: &dyn LevelReader, pos: BlockPos, state: BlockState) -> BlockState {
    let instrument_above = level.get_block_state(pos.above()).block().data().instrument;
    if works_above_note_block(instrument_above) {
        state.set(props::NOTEBLOCK_INSTRUMENT, instrument_above)
    } else {
        let instrument_below = level.get_block_state(pos.below()).block().data().instrument;
        let new_below = if works_above_note_block(instrument_below) {
            NoteBlockInstrument::Harp
        } else {
            instrument_below
        };
        state.set(props::NOTEBLOCK_INSTRUMENT, new_below)
    }
}

/// `NoteBlock.playNote`.
fn play_note(source: Option<EntityId>, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
    if works_above_note_block(state.get(props::NOTEBLOCK_INSTRUMENT))
        || level.get_block_state(pos.above()).is_air()
    {
        level.block_event(pos, state.block(), 0, 0);
        level.game_event_at_block(
            GameEvent::NOTE_BLOCK_PLAY,
            pos,
            &GameEventContext::of_entity(source),
        );
    }
}

/// `NoteBlock.getPitchFromNote`.
pub fn get_pitch_from_note(two_octave_range_note: i32) -> f32 {
    2.0f64.powf(f64::from(two_octave_range_note - 12) / 12.0) as f32
}

impl BlockBehaviour for NoteBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(set_instrument(
            &*level,
            context.get_clicked_pos(),
            block.default_state(),
        ))
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction.get_axis() == Axis::Y {
                set_instrument(&*level, pos, state)
            } else {
                state
            }
        }
    );
    fn neighbor_changed(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved_by_piston: bool,
    ) {
        let signal = level.has_neighbor_signal(pos);
        if signal != state.get(props::POWERED) {
            if signal {
                play_note(None, state, level, pos);
            }
            level.set_block_and_update(pos, state.set(props::POWERED, signal));
        }
    }
    fn use_item_on(
        &self,
        stack: &ItemStack,
        _state: BlockState,
        _level: &mut ServerLevel,
        _pos: BlockPos,
        _player: &mut ServerPlayer,
        _hand: InteractionHand,
        hit: &BlockHitResult,
    ) -> InteractionResult {
        let top_instrument = !stack.is_empty()
            && mc_data::tags::get::<mc_data::Item>("note_block_top_instruments")
                .is_some_and(|t| t.contains(stack.item()));
        if top_instrument && hit.direction == Direction::Up {
            InteractionResult::PASS
        } else {
            InteractionResult::TRY_WITH_EMPTY_HAND
        }
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        let state = state.cycle(props::NOTE);
        level.set_block_and_update(pos, state);
        play_note(Some(player.id()), state, level, pos);
        // `player.awardStat(Stats.TUNE_NOTEBLOCK)`: not implemented (`Stats::TUNE_NOTEBLOCK` exists, this block does not award it)
        InteractionResult::SUCCESS
    }
    fn attack(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
    ) {
        play_note(Some(player.id()), state, level, pos);
        // `player.awardStat(Stats.PLAY_NOTEBLOCK)`: see `use_without_item`
    }
    fn trigger_event(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _b0: i32,
        _b1: i32,
    ) -> bool {
        let instrument = state.get(props::NOTEBLOCK_INSTRUMENT);
        let data = &NOTE_BLOCK_INSTRUMENTS[instrument as usize];
        // the NOTE particle is `Level.addParticle`: client side only
        let pitch = if data.tunable {
            get_pitch_from_note(state.get(props::NOTE))
        } else {
            1.0
        };
        let custom_sound = if data.has_custom_sound {
            // `getCustomSoundId`: the head above (`SkullBlockEntity.getNoteBlockSound`)
            match level
                .get_block_entity(pos.above())
                .and_then(|be| be.get_note_block_sound())
            {
                Some(id) => Some(id),
                None => return false,
            }
        } else {
            None
        };
        let (x, y, z) = (
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );
        let seed = level.random().next_long();
        match custom_sound {
            Some(id) => level.play_seeded_variable_range_sound(
                None,
                x,
                y,
                z,
                id,
                SoundSource::Records,
                3.0,
                pitch,
                seed,
            ),
            None => level.play_seeded_sound(
                None,
                x,
                y,
                z,
                data.sound,
                SoundSource::Records,
                3.0,
                pitch,
                seed,
            ),
        }
        true
    }
}

register_block_class!("NoteBlock", |_| Box::new(NoteBlock));
