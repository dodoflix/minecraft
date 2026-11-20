//! `LegacyHoverEventFix`: converts a pre-1.16 `hoverEvent` with a `value` into the `contents` form, keeping the
//! original as a text placeholder when it is not a `show_text` event.

use crate::datafix::DataFix;
use crate::dsl::DSL;
use crate::fixes::references::References;
use crate::schema::Schema;
use crate::ty::Type;
use crate::util::read_typed_or_throw;
use crate::value::{Value, mismatch};
use mc_core::gson::{JsonElement, JsonObject};
use mc_core::math::java::{java_double_to_string, java_float_to_string};
use mc_dfu::{Dyn, Dynamic, JSON, Ops};

/// `new LegacyHoverEventFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("LegacyHoverEventFix", output_schema, false, |df| {
        let input_schema = df.input_schema();
        let input_text_component_type = input_schema.get_type(&References::TEXT_COMPONENT);
        let hover_event_type = input_text_component_type.find_field_type("hoverEvent");
        let raw_text_component_type = input_schema.get_type_raw(&References::TEXT_COMPONENT);
        let text_component_type = DSL::named_type(
            References::TEXT_COMPONENT.type_name(),
            &DSL::or_type(
                &DSL::or_type(&DSL::string(), &DSL::list_type(&raw_text_component_type)),
                &DSL::and_type4(
                    &DSL::optional_type(&DSL::field_type(
                        "extra",
                        &DSL::list_type(&raw_text_component_type),
                    )),
                    &DSL::optional_type(&DSL::field_type("separator", &raw_text_component_type)),
                    &DSL::optional_type(&DSL::field_type("hoverEvent", &hover_event_type)),
                    &DSL::remainder_type(),
                ),
            ),
        );
        if text_component_type != input_text_component_type {
            panic!(
                "java.lang.IllegalStateException: Text component type did not match, expected {text_component_type} but got {input_text_component_type}"
            );
        }
        df.fix_type_everywhere(
            "LegacyHoverEventFix",
            &text_component_type,
            move |ops, named| {
                named.map_second(|simple_or_full| {
                    map_right(simple_or_full, |full| {
                        full.map_second(|separator_hover_remainder| {
                            separator_hover_remainder.map_second(|hover_and_remainder| {
                                fix_hover_and_remainder(ops, &hover_event_type, hover_and_remainder)
                            })
                        })
                    })
                })
            },
        )
    })
}

/// `Either.mapRight`.
#[track_caller]
fn map_right(either: Value, f: impl FnOnce(Value) -> Value) -> Value {
    match either {
        Value::Right(b) => Value::right(f(*b)),
        left @ Value::Left(_) => left,
        other => mismatch("Either", &other),
    }
}

fn fix_hover_and_remainder(
    ops: &Ops,
    hover_event_type: &Type,
    hover_and_remainder: Value,
) -> Value {
    let remainder = hover_and_remainder.second().as_dynamic();
    let Some(hover_event) = remainder.get("hoverEvent").result() else {
        return hover_and_remainder;
    };
    if hover_event.get("value").result().is_none() {
        return hover_and_remainder;
    }
    let hover_action = hover_and_remainder
        .first()
        .as_option()
        .map(|hover| hover.first().as_str().to_string())
        .unwrap_or_default();
    let new_hover_event = fix_hover_event(ops, hover_event_type, &hover_action, hover_event);
    hover_and_remainder.map_first(|_| Value::left(new_hover_event))
}

fn fix_hover_event(
    ops: &Ops,
    hover_event_type: &Type,
    action: &str,
    old_hover_event: Dynamic,
) -> Value {
    if action == "show_text" {
        fix_show_text_hover(hover_event_type, &old_hover_event)
    } else {
        create_placeholder_hover(ops, hover_event_type, &old_hover_event)
    }
}

fn fix_show_text_hover(hover_event_type: &Type, old_hover_event: &Dynamic) -> Value {
    let new_hover_event = old_hover_event.rename_field("value", "contents");
    read_typed_or_throw(hover_event_type, &new_hover_event, false).value
}

fn create_placeholder_hover(
    ops: &Ops,
    hover_event_type: &Type,
    old_hover_event: &Dynamic,
) -> Value {
    let old_json = dyn_to_json(old_hover_event.convert(&JSON).value());
    let empty = Dynamic::empty(*ops);
    let contents = empty.create_map([(
        empty.create_string("text"),
        empty.create_string(format!(
            "Legacy hoverEvent: {}",
            old_json.to_stable_string()
        )),
    )]);
    let placeholder_hover_event = empty.create_map([
        (
            empty.create_string("action"),
            empty.create_string("show_text"),
        ),
        (empty.create_string("contents"), contents),
    ]);
    read_typed_or_throw(hover_event_type, &placeholder_hover_event, false).value
}

/// The text of a float or double; `GsonHelper.toStableString` writes it through a strict `JsonWriter`, which refuses
/// NaN and the infinities.
fn finite(text: String, is_finite: bool) -> String {
    if !is_finite {
        panic!("java.lang.IllegalArgumentException: Numeric values must be finite, but was {text}");
    }
    text
}

/// `Dynamic.convert(JsonOps.INSTANCE).getValue()`: the JSON element a `JsonOps` value tree stands for.
fn dyn_to_json(value: &Dyn) -> JsonElement {
    let number = |text: String| JsonElement::Number(text);
    match value {
        Dyn::Empty => JsonElement::Null,
        Dyn::Bool(b) => JsonElement::Bool(*b),
        Dyn::Byte(v) => number(v.to_string()),
        Dyn::Short(v) => number(v.to_string()),
        Dyn::Int(v) => number(v.to_string()),
        Dyn::Long(v) => number(v.to_string()),
        Dyn::Float(v) => number(finite(java_float_to_string(*v), v.is_finite())),
        Dyn::Double(v) => number(finite(java_double_to_string(*v), v.is_finite())),
        Dyn::JsonNum(text) => number(text.clone()),
        Dyn::Str(s) => JsonElement::String(s.clone()),
        Dyn::List(items) => JsonElement::Array(items.iter().map(dyn_to_json).collect()),
        Dyn::Map(entries) => {
            let mut object = JsonObject::new();
            for (key, value) in entries {
                let key = match key {
                    Dyn::Str(s) => s.clone(),
                    other => other.to_json_string(),
                };
                object.add(&key, dyn_to_json(value));
            }
            JsonElement::Object(object)
        }
        Dyn::Bytes(v) => JsonElement::Array(v.iter().map(|x| number(x.to_string())).collect()),
        Dyn::Ints(v) => JsonElement::Array(v.iter().map(|x| number(x.to_string())).collect()),
        Dyn::Longs(v) => JsonElement::Array(v.iter().map(|x| number(x.to_string())).collect()),
    }
}
