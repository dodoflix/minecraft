//! `ScoreboardDisplayNameFix`: turns the plain `DisplayName` of objectives or teams into a text component JSON.

use crate::datafix::DataFix;
use crate::dsl::{DSL, TypeReference};
use crate::fixes::references::References;
use crate::legacy_component_data_fix_utils::create_text_component_json;
use crate::schema::Schema;
use crate::value::Value;

/// `new ScoreboardDisplayNameFix(outputSchema, name, type)`.
pub fn new(output_schema: &Schema, name: &str, ty: &TypeReference) -> DataFix {
    let (name, ty) = (name.to_string(), *ty);
    DataFix::new(
        "ScoreboardDisplayNameFix",
        output_schema,
        false,
        move |df| {
            let input_type = df.input_schema().get_type(&ty);
            let display_name_f = input_type.find_field("DisplayName");
            let text_component_f =
                DSL::type_finder(&df.input_schema().get_type(&References::TEXT_COMPONENT));
            df.fix_type_everywhere_typed(&name, &input_type, move |team| {
                team.update_typed(&display_name_f, |display_name| {
                    display_name.update(&text_component_f, |text_component| {
                        text_component.map_second(|json| {
                            Value::Str(create_text_component_json(json.as_java_string().clone()))
                        })
                    })
                })
            })
        },
    )
}
