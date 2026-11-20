//! Conversions between `serde_json::Value` and the Gson model ([`JsonElement`]) the component codecs run over.
//!
//! A `serde_json::Value` cannot hold a lone UTF-16 surrogate, so reading one is exact; writing one into a `Value`
//! shows each lone surrogate as U+FFFD. Use the Gson model wherever the exact string matters.

use crate::value::java_double_to_string;
use mc_core::gson::{JsonElement, JsonObject};
use mc_core::java_string::JavaString;
use serde_json::{Map, Number, Value};

/// The Gson tree of a `serde_json` value (numbers print like Gson prints them: `Double.toString` for fractions).
pub fn element_from_value(v: &Value) -> JsonElement {
    match v {
        Value::Null => JsonElement::Null,
        Value::Bool(b) => JsonElement::Bool(*b),
        Value::Number(n) => JsonElement::Number(match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => i.to_string(),
            (None, Some(u)) => u.to_string(),
            _ => java_double_to_string(n.as_f64().unwrap_or(0.0)),
        }),
        Value::String(s) => JsonElement::String(s.as_str().into()),
        Value::Array(a) => JsonElement::Array(a.iter().map(element_from_value).collect()),
        Value::Object(o) => JsonElement::Object(JsonObject::from_distinct_members(
            o.iter()
                .map(|(k, v)| (JavaString::from(k.as_str()), element_from_value(v)))
                .collect(),
        )),
    }
}

/// The `serde_json` value of a Gson tree; each lone surrogate becomes U+FFFD.
pub fn value_from_element(e: &JsonElement) -> Value {
    match e {
        JsonElement::Null => Value::Null,
        JsonElement::Bool(b) => Value::Bool(*b),
        JsonElement::Number(t) => t
            .parse::<Number>()
            .ok()
            .or_else(|| t.parse::<f64>().ok().and_then(Number::from_f64))
            .map_or(Value::Null, Value::Number),
        JsonElement::String(s) => Value::String(s.as_lossy_str().to_owned()),
        JsonElement::Array(a) => Value::Array(a.iter().map(value_from_element).collect()),
        JsonElement::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.as_lossy_str().to_owned(), value_from_element(v)))
                .collect::<Map<_, _>>(),
        ),
    }
}
