//! `PlayerRespawnDataFix`: the player's `respawn` compound gets a default dimension and its angle becomes yaw and pitch.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new PlayerRespawnDataFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("PlayerRespawnDataFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "PlayerRespawnDataFix",
            &df.input_schema().get_type(&References::PLAYER),
            |input| {
                input.update_remainder(|tag| {
                    tag.update("respawn", |respawn_tag| {
                        respawn_tag
                            .set(
                                "dimension",
                                respawn_tag.create_string(
                                    respawn_tag
                                        .get("dimension")
                                        .as_string_or("minecraft:overworld"),
                                ),
                            )
                            .set(
                                "yaw",
                                respawn_tag.create_float(respawn_tag.get("angle").as_float_or(0.0)),
                            )
                            .set("pitch", respawn_tag.create_float(0.0))
                            .remove("angle")
                    })
                })
            },
        )
    })
}
