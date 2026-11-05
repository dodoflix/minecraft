//! `net.minecraft.world.level.block.NoteBlock`.
use super::prelude::*;
use mc_data::enums::NoteBlockInstrument;
use mc_data::misc::NOTE_BLOCK_INSTRUMENTS;

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
        let new_below = if works_above_note_block(instrument_below) { NoteBlockInstrument::Harp } else { instrument_below };
        state.set(props::NOTEBLOCK_INSTRUMENT, new_below)
    }
}

impl BlockBehaviour for NoteBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_axis() == Axis::Y { set_instrument(&*level, pos, state) } else { state }
    });
}

register_block_class!("NoteBlock", |_| Box::new(NoteBlock));
