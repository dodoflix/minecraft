//! `EntityGoatMissingStateFix`: gives goats both horns.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;

/// `new EntityGoatMissingStateFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    named_entity_fix::new(
        "EntityGoatMissingStateFix",
        output_schema,
        false,
        "EntityGoatMissingStateFix",
        &References::ENTITY,
        "minecraft:goat",
        |entity| {
            entity.update_remainder(|tag| {
                tag.set("HasLeftHorn", tag.create_boolean(true))
                    .set("HasRightHorn", tag.create_boolean(true))
            })
        },
    )
}
