//! `GoatHornIdFix`: turns the numeric `SoundVariant` of goat horns into an `instrument` id.

use crate::datafix::DataFix;
use crate::fixes::item_stack_tag_remainder_fix;
use crate::schema::Schema;

const INSTRUMENTS: [&str; 8] = [
    "minecraft:ponder_goat_horn",
    "minecraft:sing_goat_horn",
    "minecraft:seek_goat_horn",
    "minecraft:feel_goat_horn",
    "minecraft:admire_goat_horn",
    "minecraft:call_goat_horn",
    "minecraft:yearn_goat_horn",
    "minecraft:dream_goat_horn",
];

/// `new GoatHornIdFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    item_stack_tag_remainder_fix::new(
        "GoatHornIdFix",
        output_schema,
        "GoatHornIdFix",
        |id| id == "minecraft:goat_horn",
        |tag| {
            let sound_variant = tag.get("SoundVariant").as_int_or(0);
            let sound_id = usize::try_from(sound_variant)
                .ok()
                .and_then(|i| INSTRUMENTS.get(i))
                .unwrap_or(&INSTRUMENTS[0]);
            tag.remove("SoundVariant")
                .set("instrument", tag.create_string(*sound_id))
        },
    )
}
