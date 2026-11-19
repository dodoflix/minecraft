//! `net.minecraft.util.datafix.LegacyComponentDataFixUtils`: text components stored as JSON strings (pre-1.21.5).

use mc_core::gson::{self, JsonElement, JsonObject};
use mc_dfu::{Dynamic, Ops};

/// `createPlainTextComponent(ops, text)`.
pub fn create_plain_text_component(ops: Ops, text: &str) -> Dynamic {
    Dynamic::new(ops, ops.create_string(create_text_component_json(text)))
}

/// `createEmptyComponent(ops)`.
pub fn create_empty_component(ops: Ops) -> Dynamic {
    create_plain_text_component(ops, "")
}

/// `createTextComponentJson(text)`: `{"text":"<text>"}` through `GsonHelper.toStableString`.
pub fn create_text_component_json(text: &str) -> String {
    let mut result = JsonObject::new();
    result.add_property_str("text", Some(text));
    JsonElement::Object(result).to_stable_string()
}

/// `createTranslatableComponentJson(key)`.
pub fn create_translatable_component_json(key: &str) -> String {
    let mut result = JsonObject::new();
    result.add_property_str("translate", Some(key));
    JsonElement::Object(result).to_stable_string()
}

/// `createTranslatableComponent(ops, key)`.
pub fn create_translatable_component(ops: Ops, key: &str) -> Dynamic {
    Dynamic::new(
        ops,
        ops.create_string(create_translatable_component_json(key)),
    )
}

/// `rewriteFromLenient(string)`: a lenient-JSON string (quoted, object or array by its first and last character) is
/// re-serialised in stable form; anything else, or unparsable text, becomes a plain text component.
pub fn rewrite_from_lenient(string: &str) -> String {
    if string.is_empty() || string == "null" {
        return create_text_component_json("");
    }
    let first = string.chars().next();
    let last = string.chars().next_back();
    if matches!(
        (first, last),
        (Some('"'), Some('"')) | (Some('{'), Some('}')) | (Some('['), Some(']'))
    ) && let Ok(json) = gson::parse_lenient_json_parser(string)
    {
        return match &json {
            JsonElement::Bool(_) | JsonElement::Number(_) | JsonElement::String(_) => {
                create_text_component_json(&json.as_string().unwrap_or_default())
            }
            _ => json.to_stable_string(),
        };
    }
    create_text_component_json(string)
}

/// `isStrictlyValidJson(component)`: a string value that `StrictJsonParser` accepts.
pub fn is_strictly_valid_json(component: &Dynamic) -> bool {
    component
        .as_string()
        .result()
        .is_some_and(|string| gson::parse_strict_json_parser(&string).is_ok())
}

/// `extractTranslationString(component)`: the `translate` primitive of a lenient-JSON object.
pub fn extract_translation_string(component: &str) -> Option<String> {
    let Ok(JsonElement::Object(parsed)) = gson::parse_lenient_json_parser(component) else {
        return None;
    };
    match parsed.get("translate")? {
        key @ (JsonElement::Bool(_) | JsonElement::Number(_) | JsonElement::String(_)) => {
            key.as_string().ok()
        }
        _ => None,
    }
}
