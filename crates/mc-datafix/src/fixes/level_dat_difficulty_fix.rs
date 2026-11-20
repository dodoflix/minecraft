//! `LevelDatDifficultyFix`: the difficulty, hardcore and difficulty lock fields of the level data move into a
//! `difficulty_settings` compound.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new LevelDatDifficultyFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("LevelDatDifficultyFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "LevelDatDifficultyFix",
            &df.input_schema().get_type(&References::LIGHTWEIGHT_LEVEL),
            |input| {
                input.update_remainder(|level_data| {
                    let difficulty = level_data.get("Difficulty").as_int_or(2);
                    let new_difficulty = match difficulty {
                        0 => "peaceful",
                        1 => "easy",
                        3 => "hard",
                        _ => "normal",
                    };
                    let difficulty_settings = level_data
                        .empty_map()
                        .set("difficulty", level_data.create_string(new_difficulty))
                        .set(
                            "hardcore",
                            level_data
                                .create_boolean(level_data.get("hardcore").as_boolean_or(false)),
                        )
                        .set(
                            "locked",
                            level_data.create_boolean(
                                level_data.get("DifficultyLocked").as_boolean_or(false),
                            ),
                        );
                    level_data
                        .set("difficulty_settings", difficulty_settings)
                        .remove("Difficulty")
                        .remove("hardcore")
                        .remove("DifficultyLocked")
                })
            },
        )
    })
}
