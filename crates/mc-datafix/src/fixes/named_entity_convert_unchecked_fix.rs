//! `NamedEntityConvertUncheckedFix`: relabels one entity choice with its output type (the data is unchanged).

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::extra_data_fix_utils::cast;
use crate::fixes::named_entity_fix;
use crate::schema::Schema;
use std::sync::OnceLock;

/// `new NamedEntityConvertUncheckedFix(outputSchema, name, type, entityName)`.
pub fn new(output_schema: &Schema, name: &str, ty: &TypeReference, entity_name: &str) -> DataFix {
    let (schema, ty, entity) = (output_schema.clone(), *ty, entity_name.to_string());
    let output_type = OnceLock::new();
    named_entity_fix::new(
        "NamedEntityConvertUncheckedFix",
        output_schema,
        true,
        name,
        &ty,
        entity_name,
        move |typed| {
            let output_type = output_type.get_or_init(|| schema.get_choice_type(&ty, &entity));
            cast(output_type, typed)
        },
    )
}
