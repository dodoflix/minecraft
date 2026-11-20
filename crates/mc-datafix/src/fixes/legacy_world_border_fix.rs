//! `LegacyWorldBorderFix`: the `Border*` fields of the level data move into a `world_border` compound.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new LegacyWorldBorderFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("LegacyWorldBorderFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "LegacyWorldBorderFix",
            &df.input_schema().get_type(&References::LEVEL),
            |input| {
                input.update_remainder(|tag| {
                    let world_border = tag
                        .empty_map()
                        .set(
                            "center_x",
                            tag.create_double(tag.get("BorderCenterX").as_double_or(0.0)),
                        )
                        .set(
                            "center_z",
                            tag.create_double(tag.get("BorderCenterZ").as_double_or(0.0)),
                        )
                        .set(
                            "size",
                            tag.create_double(
                                tag.get("BorderSize")
                                    .as_double_or(f64::from(5.999997E7_f32)),
                            ),
                        )
                        .set(
                            "lerp_time",
                            tag.create_long(tag.get("BorderSizeLerpTime").as_long_or(0)),
                        )
                        .set(
                            "lerp_target",
                            tag.create_double(tag.get("BorderSizeLerpTarget").as_double_or(0.0)),
                        )
                        .set(
                            "safe_zone",
                            tag.create_double(tag.get("BorderSafeZone").as_double_or(5.0)),
                        )
                        .set(
                            "damage_per_block",
                            tag.create_double(tag.get("BorderDamagePerBlock").as_double_or(0.2)),
                        )
                        .set(
                            "warning_blocks",
                            tag.create_int(tag.get("BorderWarningBlocks").as_int_or(5)),
                        )
                        .set(
                            "warning_time",
                            tag.create_int(tag.get("BorderWarningTime").as_int_or(15)),
                        );
                    tag.remove("BorderCenterX")
                        .remove("BorderCenterZ")
                        .remove("BorderSize")
                        .remove("BorderSizeLerpTime")
                        .remove("BorderSizeLerpTarget")
                        .remove("BorderSafeZone")
                        .remove("BorderDamagePerBlock")
                        .remove("BorderWarningBlocks")
                        .remove("BorderWarningTime")
                        .set("world_border", world_border)
                })
            },
        )
    })
}
