//! `EntityElderGuardianSplitFix`: renames elder guardians to `ElderGuardian`.

use crate::datafix::DataFix;
use crate::fixes::simple_entity_rename_fix;
use crate::schema::Schema;

/// `new EntityElderGuardianSplitFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    simple_entity_rename_fix::new(
        "EntityElderGuardianSplitFix",
        "EntityElderGuardianSplitFix",
        output_schema,
        changes_type,
        |name, tag| {
            let new_name = if name == "Guardian" && tag.get("Elder").as_boolean_or(false) {
                "ElderGuardian"
            } else {
                name
            };
            (new_name.to_string(), tag)
        },
    )
}
