//! `NamedEntityWriteReadFix`: like `NamedEntityFix`, but the entity is written to a `Dynamic`, fixed there and read
//! back as the output type. `fix` is the subclass's `fix(Dynamic)`.

use crate::datafix::DataFix;
use crate::dsl::{DSL, TypeReference};
use crate::extra_data_fix_utils::{cast, patch_sub_type};
use crate::schema::Schema;
use crate::util::write_and_read_typed_or_throw;
use mc_dfu::Dynamic;
use std::sync::Arc;

/// `new NamedEntityWriteReadFix(outputSchema, changesType, name, type, entityName)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    changes_type: bool,
    name: &str,
    ty: &TypeReference,
    entity_name: &str,
    fix: impl Fn(Dynamic) -> Dynamic + Send + Sync + 'static,
) -> DataFix {
    let (name, ty, entity_name) = (name.to_string(), *ty, entity_name.to_string());
    let fix = Arc::new(fix);
    DataFix::new(class, output_schema, changes_type, move |df| {
        let input_entity_type = df.input_schema().get_type(&ty);
        let input_entity_choice_type = df.input_schema().get_choice_type(&ty, &entity_name);
        let output_entity_type = df.output_schema().get_type(&ty);
        let entity_f = DSL::named_choice(&entity_name, &input_entity_choice_type);
        let patched_entity_type =
            patch_sub_type(&input_entity_type, &input_entity_type, &output_entity_type);
        let (fix, output_entity_type2) = (fix.clone(), output_entity_type.clone());
        df.fix_type_everywhere_typed_new(
            &name,
            &input_entity_type,
            &output_entity_type,
            move |typed| {
                if typed.get_optional(&entity_f).is_none() {
                    cast(&output_entity_type2, typed)
                } else {
                    let fake_typed = cast(&patched_entity_type, typed);
                    write_and_read_typed_or_throw(&fake_typed, &output_entity_type2, |d| fix(d))
                }
            },
        )
    })
}
