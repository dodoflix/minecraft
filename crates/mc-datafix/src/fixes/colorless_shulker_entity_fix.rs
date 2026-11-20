//! `ColorlessShulkerEntityFix`: shulkers with the (colorless) color 10 become color 16.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new ColorlessShulkerEntityFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    named_entity_fix::new(
        "ColorlessShulkerEntityFix",
        output_schema,
        changes_type,
        "Colorless shulker entity fix",
        &References::ENTITY,
        "minecraft:shulker",
        |entity| {
            entity.update_remainder(|tag| {
                if tag.get("Color").as_int_or(0) == 10 {
                    tag.set("Color", tag.create_byte(16))
                } else {
                    tag
                }
            })
        },
    )
}
