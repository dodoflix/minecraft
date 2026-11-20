//! `VariantRenameFix`: renames the `variant` values of one entity type.

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::fixes::named_entity_fix;
use crate::schema::Schema;

/// `new VariantRenameFix(outputSchema, name, type, entityName, renames)`; `renames` are the `(old, new)` entries.
pub fn new(
    output_schema: &Schema,
    name: &str,
    ty: &TypeReference,
    entity_name: &str,
    renames: &[(&str, &str)],
) -> DataFix {
    let renames: Vec<(String, String)> = renames
        .iter()
        .map(|(old, new)| (old.to_string(), new.to_string()))
        .collect();
    named_entity_fix::new(
        "VariantRenameFix",
        output_schema,
        false,
        name,
        ty,
        entity_name,
        move |typed| {
            typed.update_remainder(|remainder| {
                remainder.update("variant", |variant| match variant.as_string().result() {
                    Some(v) => {
                        let renamed = renames
                            .iter()
                            .find(|(old, _)| *old == v)
                            .map_or(v.as_str(), |(_, new)| new.as_str());
                        variant.create_string(renamed)
                    }
                    None => variant,
                })
            })
        },
    )
}
