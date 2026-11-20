//! `LegacyDragonFightFix`: moves the dragon fight of the end from the dimension data to the level.

use crate::datafix::DataFix;
use crate::extra_data_fix_utils::fix_block_pos;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

fn fix_dragon_fight(tag: Dynamic) -> Dynamic {
    tag.update("ExitPortalLocation", |pos| fix_block_pos(&pos))
}

/// `new LegacyDragonFightFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("LegacyDragonFightFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "LegacyDragonFightFix",
            &df.input_schema().get_type(&References::LEVEL),
            |input| {
                input.update_remainder(|tag| {
                    if tag.get("DragonFight").result().is_some() {
                        tag
                    } else {
                        let legacy_fight = tag
                            .get("DimensionData")
                            .get("1")
                            .get("DragonFight")
                            .or_else_empty_map();
                        tag.set("DragonFight", fix_dragon_fight(legacy_fight))
                    }
                })
            },
        )
    })
}
