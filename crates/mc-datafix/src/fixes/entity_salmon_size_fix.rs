//! `EntitySalmonSizeFix`: salmon sizes other than `large` become `medium`.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new EntitySalmonSizeFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "EntitySalmonSizeFix",
        output_schema,
        false,
        "EntitySalmonSizeFix",
        &References::ENTITY,
        "minecraft:salmon",
        |entity| {
            entity.update_remainder(|tag| {
                let ty = tag.get("type").as_string_or("medium");
                if ty == "large" {
                    tag
                } else {
                    tag.set("type", tag.create_string("medium"))
                }
            })
        },
    )
}
