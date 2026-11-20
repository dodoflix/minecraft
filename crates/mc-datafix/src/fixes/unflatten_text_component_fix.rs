//! `UnflattenTextComponentFix`: turns the JSON string of a text component into the structured component.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::util::read_typed_or_throw;
use mc_core::gson::{self, JsonElement};
use mc_dfu::{Dyn, Dynamic, JSON, Ops};

/// `new UnflattenTextComponentFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("UnflattenTextComponentFix", output_schema, true, |df| {
        let text_component_type = df.input_schema().get_type(&References::TEXT_COMPONENT);
        let new_text_component_type = df.output_schema().get_type(&References::TEXT_COMPONENT);
        let new_type = new_text_component_type.clone();
        df.fix_type_everywhere_new(
            "UnflattenTextComponentFix",
            &text_component_type,
            &new_text_component_type,
            move |ops, input| {
                read_typed_or_throw(
                    &new_type,
                    &unflatten_json(ops, input.second().as_str()),
                    true,
                )
                .value
            },
        )
    })
}

fn unflatten_json(ops: &Ops, json_string: &str) -> Dynamic {
    match gson::parse_lenient_json_parser(json_string) {
        Ok(JsonElement::Null) => {}
        // `JsonOps.convertTo` dereferences the `null` it is handed for a nested `JsonNull` and reads every number as a
        // `BigDecimal`, which refuses the texts beyond Gson's `NumberLimits`: both throw into the `catch (Exception)`
        Ok(json) if convert_throws(&json) => {
            log::error!("Failed to unflatten text component json: {json_string}");
        }
        Ok(json) => return Dynamic::new(*ops, JSON.convert_to(ops, &json_to_dyn(&json))),
        // slf4j formats the message without the trailing throwable
        Err(_) => log::error!("Failed to unflatten text component json: {json_string}"),
    }
    Dynamic::new(*ops, ops.create_string(json_string))
}

fn convert_throws(json: &JsonElement) -> bool {
    match json {
        JsonElement::Number(text) => mc_core::gson::lazily_parsed_int_value(text).is_err(),
        JsonElement::Array(items) => items
            .iter()
            .any(|e| matches!(e, JsonElement::Null) || convert_throws(e)),
        JsonElement::Object(object) => object
            .iter()
            .any(|(_, e)| matches!(e, JsonElement::Null) || convert_throws(e)),
        _ => false,
    }
}

/// The value tree `JsonOps` stands for the parsed element with.
fn json_to_dyn(json: &JsonElement) -> Dyn {
    match json {
        JsonElement::Null => Dyn::Empty,
        JsonElement::Bool(b) => Dyn::Bool(*b),
        JsonElement::Number(text) => Dyn::JsonNum(text.clone()),
        JsonElement::String(s) => Dyn::Str(s.clone()),
        JsonElement::Array(items) => Dyn::List(items.iter().map(json_to_dyn).collect()),
        JsonElement::Object(object) => Dyn::Map(
            object
                .iter()
                .map(|(k, v)| (Dyn::str(k), json_to_dyn(v)))
                .collect(),
        ),
    }
}
