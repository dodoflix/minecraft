//! `JukeboxTicksSinceSongStartedFix`: replaces the tick counters of a jukebox by the ticks since its song started.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new JukeboxTicksSinceSongStartedFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "JukeboxTicksSinceSongStartedFix",
        output_schema,
        false,
        "JukeboxTicksSinceSongStartedFix",
        &References::BLOCK_ENTITY,
        "minecraft:jukebox",
        |entity| entity.update_remainder(fix_tag),
    )
}

fn fix_tag(input: Dynamic) -> Dynamic {
    let ticks_since_song_started = input
        .get("TickCount")
        .as_long_or(0)
        .wrapping_sub(input.get("RecordStartTick").as_long_or(0));
    let result = input
        .remove("IsPlaying")
        .remove("TickCount")
        .remove("RecordStartTick");
    if ticks_since_song_started > 0 {
        result.set(
            "ticks_since_song_started",
            input.create_long(ticks_since_song_started),
        )
    } else {
        result
    }
}
