//! `ScoreboardDisplaySlotFix`: renames the numbered display slots of the scoreboard.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

const SLOT_RENAMES: [(&str, &str); 19] = [
    ("slot_0", "list"),
    ("slot_1", "sidebar"),
    ("slot_2", "below_name"),
    ("slot_3", "sidebar.team.black"),
    ("slot_4", "sidebar.team.dark_blue"),
    ("slot_5", "sidebar.team.dark_green"),
    ("slot_6", "sidebar.team.dark_aqua"),
    ("slot_7", "sidebar.team.dark_red"),
    ("slot_8", "sidebar.team.dark_purple"),
    ("slot_9", "sidebar.team.gold"),
    ("slot_10", "sidebar.team.gray"),
    ("slot_11", "sidebar.team.dark_gray"),
    ("slot_12", "sidebar.team.blue"),
    ("slot_13", "sidebar.team.green"),
    ("slot_14", "sidebar.team.aqua"),
    ("slot_15", "sidebar.team.red"),
    ("slot_16", "sidebar.team.light_purple"),
    ("slot_17", "sidebar.team.yellow"),
    ("slot_18", "sidebar.team.white"),
];

fn rename(key: &str) -> Option<&'static str> {
    SLOT_RENAMES
        .iter()
        .find(|(from, _)| *from == key)
        .map(|(_, to)| *to)
}

/// `new ScoreboardDisplaySlotFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("ScoreboardDisplaySlotFix", output_schema, false, |df| {
        let scoreboard_type = df
            .input_schema()
            .get_type(&References::SAVED_DATA_SCOREBOARD);
        let root_tag_finder = scoreboard_type.find_field("data");
        df.fix_type_everywhere_typed(
            "Scoreboard DisplaySlot rename",
            &scoreboard_type,
            move |input| {
                input.update_typed(&root_tag_finder, |scoreboard_info| {
                    scoreboard_info.update_remainder(|tag| {
                        tag.update("DisplaySlots", |slots| {
                            slots.update_map_values(|(key, value)| {
                                let renamed = key
                                    .as_string()
                                    .result()
                                    .and_then(|k| rename(&k).map(|to| key.create_string(to)))
                                    .unwrap_or(key);
                                (renamed, value)
                            })
                        })
                    })
                })
            },
        )
    })
}
