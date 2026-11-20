//! `EntityTippedArrowFix`: renames the `TippedArrow` entity to `Arrow`.

use crate::datafix::DataFix;
use crate::fixes::simplest_entity_rename_fix;
use crate::schema::Schema;

/// `new EntityTippedArrowFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    simplest_entity_rename_fix::new(
        "EntityTippedArrowFix",
        "EntityTippedArrowFix",
        output_schema,
        changes_type,
        |name| {
            if name == "TippedArrow" {
                "Arrow".to_string()
            } else {
                name.to_string()
            }
        },
    )
}
