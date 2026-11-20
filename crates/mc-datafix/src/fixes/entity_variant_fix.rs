//! `EntityVariantFix`: replaces a numeric variant id of one entity by its string id, stored under `variant`.

use crate::datafix::DataFix;
use crate::dsl::TypeReference;
use crate::fixes::named_entity_fix;
use crate::schema::Schema;
use mc_dfu::Dynamic;

/// `EntityVariantFix.updateAndRename(input, oldKey, newKey, function)`: the old key stays.
fn update_and_rename(
    input: Dynamic,
    old_key: &str,
    new_key: &str,
    function: impl FnOnce(Dynamic) -> Dynamic,
) -> Dynamic {
    match input.get(old_key).result() {
        Some(field_value) => input.set(new_key, function(field_value)),
        None => input,
    }
}

/// `new EntityVariantFix(outputSchema, name, type, entityName, fieldName, idConversions)`; `id_conversions` is the
/// `IntFunction<String>` (`None` is a Java `null` result).
pub fn new(
    output_schema: &Schema,
    name: &str,
    ty: &TypeReference,
    entity_name: &str,
    field_name: &str,
    id_conversions: impl Fn(i32) -> Option<String> + Send + Sync + 'static,
) -> DataFix {
    let field_name = field_name.to_string();
    named_entity_fix::new(
        "EntityVariantFix",
        output_schema,
        false,
        name,
        ty,
        entity_name,
        move |typed| {
            typed.update_remainder(|remainder| {
                update_and_rename(remainder, &field_name, "variant", |cat_type| {
                    match cat_type.as_number().result() {
                        Some(e) => {
                            let id = id_conversions(e.int_value()).unwrap_or_else(|| {
                                panic!("java.lang.NullPointerException: Cannot invoke \"String.isEmpty()\" because \"data\" is null")
                            });
                            cat_type.create_string(id)
                        }
                        None => cat_type,
                    }
                })
            })
        },
    )
}
