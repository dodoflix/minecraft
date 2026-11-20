//! `LegacyDimensionIdFix`: turns the numeric dimension ids of players and map data into dimension names.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::rewrite::TypeRewriteRule;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new LegacyDimensionIdFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("LegacyDimensionIdFix", output_schema, false, |df| {
        let player_rule = df.fix_type_everywhere_typed(
            "PlayerLegacyDimensionFix",
            &df.input_schema().get_type(&References::PLAYER),
            |input| input.update_remainder(fix_player),
        );
        let data_type = df.input_schema().get_type(&References::SAVED_DATA_MAP_DATA);
        let map_data_f = data_type.find_field("data");
        let map_rule =
            df.fix_type_everywhere_typed("MapLegacyDimensionFix", &data_type, move |input| {
                input.update_typed(&map_data_f, |data| data.update_remainder(fix_map))
            });
        TypeRewriteRule::seq(player_rule, map_rule)
    })
}

fn fix_map(remainder: Dynamic) -> Dynamic {
    remainder.update("dimension", fix_dimension_id)
}

fn fix_player(remainder: Dynamic) -> Dynamic {
    remainder.update("Dimension", fix_dimension_id)
}

fn fix_dimension_id(id: Dynamic) -> Dynamic {
    match id.as_number().result() {
        Some(legacy_id) => id.create_string(match legacy_id.int_value() {
            -1 => "minecraft:the_nether",
            1 => "minecraft:the_end",
            _ => "minecraft:overworld",
        }),
        None => id,
    }
}
