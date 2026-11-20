//! `EntityAttributeBaseValueFix`: rewrites the base value of one attribute of one entity id with a function.

use crate::datafix::DataFix;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::ensure_namespaced;
use mc_dfu::Dynamic;

/// `new EntityAttributeBaseValueFix(outputSchema, name, entityName, attributeId, valueFixer)`.
pub fn new(
    output_schema: &Schema,
    name: &str,
    entity_name: &str,
    attribute_id: &str,
    value_fixer: impl Fn(f64) -> f64 + Send + Sync + 'static,
) -> DataFix {
    let attribute_id = attribute_id.to_string();
    named_entity_fix::new(
        "EntityAttributeBaseValueFix",
        output_schema,
        false,
        name,
        &References::ENTITY,
        entity_name,
        move |entity| entity.update_remainder(|tag| fix_value(tag, &attribute_id, &value_fixer)),
    )
}

fn fix_value(tag: Dynamic, wanted_id: &str, value_fixer: &impl Fn(f64) -> f64) -> Dynamic {
    tag.update("attributes", |attributes| {
        tag.create_list(attributes.as_stream().into_iter().map(|attribute| {
            let attribute_id =
                ensure_namespaced(attribute.get("id").as_string_or("").as_lossy_str());
            if attribute_id != wanted_id {
                attribute
            } else {
                let base = attribute.get("base").as_double_or(0.0);
                attribute.set("base", attribute.create_double(value_fixer(base)))
            }
        }))
    })
}
