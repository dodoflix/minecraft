//! `NamedEntityFix`: rewrites the entities (or block entities) of one id. Abstract in Java; `fix` is the subclass's
//! `fix(entity)`. A subclass that needs `getInputSchema()` / `getOutputSchema()` inside `fix` captures its own schema
//! clones (the input schema is the output schema's parent when `changes_type`).

use crate::datafix::DataFix;
use crate::dsl::{DSL, TypeReference};
use crate::schema::Schema;
use crate::typed::Typed;
use std::sync::{Arc, OnceLock};

/// `new NamedEntityFix(outputSchema, changesType, name, type, entityName)` of the subclass `class`.
pub fn new(
    class: &str,
    output_schema: &Schema,
    changes_type: bool,
    name: &str,
    ty: &TypeReference,
    entity_name: &str,
    fix: impl Fn(Typed) -> Typed + Send + Sync + 'static,
) -> DataFix {
    let (name, ty, entity_name) = (name.to_string(), *ty, entity_name.to_string());
    let fix = Arc::new(fix);
    DataFix::new(class, output_schema, changes_type, move |df| {
        let (input_schema, output_schema) = (df.input_schema(), df.output_schema());
        let entity_f = DSL::named_choice(
            &entity_name,
            &input_schema.get_choice_type(&ty, &entity_name),
        );
        // Java resolves the output choice type inside the per-value lambda, so a missing choice fails on the first value
        let output_choice: OnceLock<_> = OnceLock::new();
        let (fix, entity_name, output_schema2) =
            (fix.clone(), entity_name.clone(), output_schema.clone());
        df.fix_type_everywhere_typed_new(
            &name,
            &input_schema.get_type(&ty),
            &output_schema.get_type(&ty),
            move |input| {
                let choice =
                    output_choice.get_or_init(|| output_schema2.get_choice_type(&ty, &entity_name));
                input.update_typed_as(&entity_f, choice, |entity| fix(entity))
            },
        )
    })
}
