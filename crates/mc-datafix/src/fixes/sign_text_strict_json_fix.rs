//! `SignTextStrictJsonFix`: rewrites the lenient JSON of the four sign lines to strict JSON.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::named_entity_fix;
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::rewrite_from_lenient;
use crate::schema::Schema;
use crate::value::Value;

const LINE_FIELDS: [&str; 4] = ["Text1", "Text2", "Text3", "Text4"];

/// `new SignTextStrictJsonFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    // changesType is false: the input schema is the output schema
    let input_schema = output_schema.clone();
    named_entity_fix::new(
        "SignTextStrictJsonFix",
        output_schema,
        false,
        "SignTextStrictJsonFix",
        &References::BLOCK_ENTITY,
        "Sign",
        move |mut entity| {
            for line_field in LINE_FIELDS {
                let line_f = entity.get_type().find_field(line_field);
                let text_component_f =
                    DSL::type_finder(&input_schema.get_type(&References::TEXT_COMPONENT));
                entity = entity.update_typed(&line_f, |line| {
                    line.update(&text_component_f, |text_component| {
                        text_component
                            .map_second(|json| Value::Str(rewrite_from_lenient(json.as_str())))
                    })
                });
            }
            entity
        },
    )
}
