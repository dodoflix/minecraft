//! The `GsonHelper` accessors the text filter configuration and the moderation answers are read with, with the
//! exceptions (class and text) vanilla throws, so a log line shows what vanilla's shows.

use mc_core::gson::{JsonElement, JsonObject};
use mc_core::java_exception::JavaException;

const JSON_SYNTAX: &str = "com.google.gson.JsonSyntaxException";

pub fn syntax(message: impl Into<String>) -> JavaException {
    JavaException::unchecked(JSON_SYNTAX, message)
}

/// `StringUtils.abbreviateMiddle(value, "...", 10)`.
fn abbreviate_middle(value: &str) -> String {
    let units: Vec<u16> = value.encode_utf16().collect();
    if units.len() <= 10 {
        return value.to_owned();
    }
    let end = units.len() - 3;
    format!(
        "{}...{}",
        String::from_utf16_lossy(&units[..4]),
        String::from_utf16_lossy(&units[end..])
    )
}

fn is_primitive(element: &JsonElement) -> bool {
    matches!(
        element,
        JsonElement::Bool(_) | JsonElement::Number(_) | JsonElement::String(_)
    )
}

/// `GsonHelper.getType`.
fn get_type(element: &JsonElement) -> String {
    let value = abbreviate_middle(&element.to_compact_string());
    match element {
        JsonElement::Null => "null (json)".to_owned(),
        JsonElement::Array(_) => format!("an array ({value})"),
        JsonElement::Object(_) => format!("an object ({value})"),
        JsonElement::Number(_) => format!("a number ({value})"),
        JsonElement::Bool(_) => format!("a boolean ({value})"),
        JsonElement::String(_) => value,
    }
}

/// `convertToString`.
pub fn convert_to_string(element: &JsonElement, name: &str) -> Result<String, JavaException> {
    if is_primitive(element) {
        Ok(element
            .as_string()
            .map_err(|e| syntax(e.0))?
            .as_lossy_str()
            .to_owned())
    } else {
        Err(syntax(format!(
            "Expected {name} to be a string, was {}",
            get_type(element)
        )))
    }
}

/// `getAsString(object, name)`.
pub fn get_as_string(object: &JsonObject, name: &str) -> Result<String, JavaException> {
    match object.get(name) {
        Some(element) => convert_to_string(element, name),
        None => Err(syntax(format!("Missing {name}, expected to find a string"))),
    }
}

/// `getAsString(object, name, def)`.
pub fn get_as_string_or(
    object: &JsonObject,
    name: &str,
    default: &str,
) -> Result<String, JavaException> {
    match object.get(name) {
        Some(element) => convert_to_string(element, name),
        None => Ok(default.to_owned()),
    }
}

/// `getAsInt(object, name, def)`.
pub fn get_as_int_or(object: &JsonObject, name: &str, default: i32) -> Result<i32, JavaException> {
    match object.get(name) {
        None => Ok(default),
        Some(element @ JsonElement::Number(_)) => get_as_int(element),
        Some(element) => Err(syntax(format!(
            "Expected {name} to be a Int, was {}",
            get_type(element)
        ))),
    }
}

/// `getAsBoolean(object, name, def)`.
pub fn get_as_bool_or(
    object: &JsonObject,
    name: &str,
    default: bool,
) -> Result<bool, JavaException> {
    match object.get(name) {
        None => Ok(default),
        Some(element) if is_primitive(element) => element.as_bool().map_err(|e| syntax(e.0)),
        Some(element) => Err(syntax(format!(
            "Expected {name} to be a Boolean, was {}",
            get_type(element)
        ))),
    }
}

/// `getAsJsonObject(object, name, null)`.
pub fn get_as_object_opt<'a>(
    object: &'a JsonObject,
    name: &str,
) -> Result<Option<&'a JsonObject>, JavaException> {
    match object.get(name) {
        None => Ok(None),
        Some(JsonElement::Object(o)) => Ok(Some(o)),
        Some(element) => Err(syntax(format!(
            "Expected {name} to be a JsonObject, was {}",
            get_type(element)
        ))),
    }
}

/// `getAsJsonArray(object, name)`.
pub fn get_as_array<'a>(
    object: &'a JsonObject,
    name: &str,
) -> Result<&'a [JsonElement], JavaException> {
    match object.get(name) {
        Some(JsonElement::Array(a)) => Ok(a),
        Some(element) => Err(syntax(format!(
            "Expected {name} to be a JsonArray, was {}",
            get_type(element)
        ))),
        None => Err(syntax(format!(
            "Missing {name}, expected to find a JsonArray"
        ))),
    }
}

/// `getAsJsonArray(object, name, new JsonArray())`.
pub fn get_as_array_or_empty<'a>(
    object: &'a JsonObject,
    name: &str,
) -> Result<&'a [JsonElement], JavaException> {
    match object.get(name) {
        None => Ok(&[]),
        Some(_) => get_as_array(object, name),
    }
}

/// `JsonElement.getAsInt()`.
pub fn get_as_int(element: &JsonElement) -> Result<i32, JavaException> {
    match element {
        JsonElement::Array(items) if items.len() != 1 => Err(JavaException::unchecked(
            "java.lang.IllegalStateException",
            format!("Array must have size 1, but has size {}", items.len()),
        )),
        JsonElement::Null => Err(JavaException::unchecked(
            "java.lang.UnsupportedOperationException",
            "JsonNull",
        )),
        JsonElement::Object(_) => Err(JavaException::unchecked(
            "java.lang.UnsupportedOperationException",
            "JsonObject",
        )),
        _ => element
            .as_int()
            .map_err(|e| JavaException::unchecked("java.lang.NumberFormatException", e.0)),
    }
}

/// `JsonElement.getAsJsonObject()`.
pub fn as_object(element: &JsonElement) -> Result<&JsonObject, JavaException> {
    element.as_object().ok_or_else(|| {
        JavaException::unchecked(
            "java.lang.IllegalStateException",
            format!("Not a JSON Object: {}", element.to_compact_string()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::gson::parse_lenient_json_parser;

    fn object(text: &str) -> JsonObject {
        match parse_lenient_json_parser(text).unwrap() {
            JsonElement::Object(o) => o,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn accessors_throw_vanillas_texts() {
        let o = object(r#"{"a":"text that is long","n":5,"b":true,"l":[1]}"#);
        assert_eq!(
            get_as_string(&o, "x").unwrap_err().to_string(),
            "com.google.gson.JsonSyntaxException: Missing x, expected to find a string"
        );
        assert_eq!(
            get_as_string(&o, "l").unwrap_err().message.unwrap(),
            "Expected l to be a string, was an array ([1])"
        );
        assert_eq!(
            get_as_int_or(&o, "a", 1).unwrap_err().message.unwrap(),
            "Expected a to be a Int, was \"tex...ng\""
        );
        assert_eq!(get_as_int_or(&o, "n", 1).unwrap(), 5);
        assert_eq!(get_as_int_or(&o, "x", 7).unwrap(), 7);
        assert!(get_as_bool_or(&o, "b", false).unwrap());
        assert_eq!(
            get_as_array(&o, "b").unwrap_err().message.unwrap(),
            "Expected b to be a JsonArray, was a boolean (true)"
        );
        assert_eq!(
            as_object(&JsonElement::Array(vec![]))
                .unwrap_err()
                .to_string(),
            "java.lang.IllegalStateException: Not a JSON Object: []"
        );
    }
}
