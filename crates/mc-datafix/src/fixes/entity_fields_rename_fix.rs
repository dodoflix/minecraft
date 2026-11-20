//! `EntityFieldsRenameFix`: renames fields in the remainder of one entity type.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `new EntityFieldsRenameFix(outputSchema, name, entityType, renames)`; `renames` are the `(old, new)` entries.
pub fn new(
    output_schema: &Schema,
    name: &str,
    entity_type: &str,
    renames: &[(&str, &str)],
) -> DataFix {
    let renames: Vec<(String, String)> = renames
        .iter()
        .map(|(old, new)| (old.to_string(), new.to_string()))
        .collect();
    named_entity_fix::new(
        "EntityFieldsRenameFix",
        output_schema,
        false,
        name,
        &References::ENTITY,
        entity_type,
        move |entity| {
            entity.update_remainder(|data| {
                renames.iter().fold(data, |data: Dynamic, (old, new)| {
                    data.rename_field(old, new)
                })
            })
        },
    )
}
