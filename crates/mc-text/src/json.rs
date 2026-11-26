//! Conversions between `serde_json::Value` and the Gson model ([`JsonElement`]) the component codecs run over.
//!
//! A `serde_json::Value` cannot hold a lone UTF-16 surrogate, so reading one is exact; writing one into a `Value`
//! shows each lone surrogate as U+FFFD. Use the Gson model wherever the exact string matters.

use crate::value::java_double_to_string;
use mc_core::gson::{JsonElement, JsonObject, lazy_number_text, lazy_number_value};
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
        Value::Object(_) if lazy_number_text(v).is_some() => {
            JsonElement::Number(lazy_number_text(v).unwrap_or_default().to_owned())
        }
        Value::Object(o) => JsonElement::Object(JsonObject::from_distinct_members(
            o.iter()
                .map(|(k, v)| (JavaString::from(k.as_str()), element_from_value(v)))
                .collect(),
        )),
    }
}

/// The `serde_json` value of a Gson tree; each lone surrogate becomes U+FFFD and a number `serde_json` would change (beyond
/// `f64` such as `1e400`, an integer beyond 64 bits, past Gson's limits) a [`lazy_number_value`].
pub fn value_from_element(e: &JsonElement) -> Value {
    match e {
        JsonElement::Null => Value::Null,
        JsonElement::Bool(b) => Value::Bool(*b),
        JsonElement::Number(t) => {
            plain_number(t).map_or_else(|| lazy_number_value(t), Value::Number)
        }
        JsonElement::String(s) => Value::String(s.as_lossy_str().to_owned()),
        JsonElement::Array(a) => Value::Array(a.iter().map(value_from_element).collect()),
        JsonElement::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.as_lossy_str().to_owned(), value_from_element(v)))
                .collect::<Map<_, _>>(),
        ),
    }
}

/// `text` as a `serde_json` number unless that changes what Gson's `LazilyParsedNumber` reads from it: a number beyond `f64`,
/// one past Gson's limits (`1e-10000` would read as 0) or digits an `f64` rounds away (an int beyond 64 bits).
fn plain_number(text: &str) -> Option<Number> {
    let n: Number = text.parse().ok()?;
    let exact = !n.is_f64() || {
        use mc_core::gson::lazily_parsed_long_value as long_value;
        long_value(text) == long_value(&n.to_string())
    };
    exact.then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_serde_json_would_change_stay_stand_ins() {
        for text in [
            "1e400",
            "1e-10000",
            "123456789012345678901",
            "-9223372036854775809",
            "0.99999999999999999999",
        ] {
            let v = value_from_element(&JsonElement::Number(text.to_owned()));
            assert_eq!(mc_core::gson::lazy_number_text(&v), Some(text), "{text}");
        }
        for text in ["7", "-0", "-0.5", "1e10", "18446744073709551615"] {
            let v = value_from_element(&JsonElement::Number(text.to_owned()));
            assert!(v.is_number(), "{text}");
        }
    }
}
