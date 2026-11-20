//! `AbstractUUIDFix`: the helpers that turn the old UUID encodings (string, `M`/`L`, `<key>Most`/`<key>Least`) into
//! the int array. The subclasses build their own `DataFix` (`new DataFix(outputSchema, false)`) and keep their
//! `typeReference`.

use crate::dsl::TypeReference;
use crate::schema::Schema;
use crate::typed::Typed;
use mc_core::uuid_util::{least_significant_bits, most_significant_bits, uuid_from_string};
use mc_dfu::Dynamic;

/// `updateNamedChoice(input, name, function)`: `function` on the remainder of choice `name` of `type_reference`.
/// `schema` is the fix's schema (`changesType` is false: the input and output schemas are the same).
pub fn update_named_choice(
    schema: &Schema,
    type_reference: &TypeReference,
    input: Typed,
    name: &str,
    function: impl Fn(Dynamic) -> Dynamic,
) -> Typed {
    let old_type = schema.get_choice_type(type_reference, name);
    let new_type = schema.get_choice_type(type_reference, name);
    input.update_typed_as(
        &crate::dsl::DSL::named_choice(name, &old_type),
        &new_type,
        |typed_tag| typed_tag.update_remainder(&function),
    )
}

/// `replaceUUIDString(tag, oldKey, newKey)`.
pub fn replace_uuid_string(tag: &Dynamic, old_key: &str, new_key: &str) -> Option<Dynamic> {
    create_uuid_from_string(tag, old_key).map(|uuid_tag| tag.remove(old_key).set(new_key, uuid_tag))
}

/// `replaceUUIDMLTag(tag, oldKey, newKey)`.
pub fn replace_uuid_ml_tag(tag: &Dynamic, old_key: &str, new_key: &str) -> Option<Dynamic> {
    tag.get(old_key)
        .result()
        .and_then(|d| create_uuid_from_ml(&d))
        .map(|uuid_tag| tag.remove(old_key).set(new_key, uuid_tag))
}

/// `replaceUUIDLeastMost(tag, oldKey, newKey)`.
pub fn replace_uuid_least_most(tag: &Dynamic, old_key: &str, new_key: &str) -> Option<Dynamic> {
    let most_key = format!("{old_key}Most");
    let least_key = format!("{old_key}Least");
    create_uuid_from_longs(tag, &most_key, &least_key).map(|uuid_tag| {
        tag.remove(&most_key)
            .remove(&least_key)
            .set(new_key, uuid_tag)
    })
}

/// `createUUIDFromString(tag, oldKey)`: `UUID.fromString` of the string at `oldKey` (a malformed string gives `None`).
pub fn create_uuid_from_string(tag: &Dynamic, old_key: &str) -> Option<Dynamic> {
    let uuid_string = tag.get(old_key).result()?.as_string().result()?;
    let uuid = uuid_from_string(uuid_string.as_lossy_str()).ok()?;
    Some(create_uuid_tag(
        tag,
        most_significant_bits(&uuid),
        least_significant_bits(&uuid),
    ))
}

/// `createUUIDFromML(tag)`.
pub fn create_uuid_from_ml(tag: &Dynamic) -> Option<Dynamic> {
    create_uuid_from_longs(tag, "M", "L")
}

/// `createUUIDFromLongs(tag, mostKey, leastKey)`: `None` when either half is 0 or missing.
pub fn create_uuid_from_longs(tag: &Dynamic, most_key: &str, least_key: &str) -> Option<Dynamic> {
    let most = tag.get(most_key).as_long_or(0);
    let least = tag.get(least_key).as_long_or(0);
    (most != 0 && least != 0).then(|| create_uuid_tag(tag, most, least))
}

/// `createUUIDTag(tag, mostSignificantBits, leastSignificantBits)`.
pub fn create_uuid_tag(tag: &Dynamic, most: i64, least: i64) -> Dynamic {
    tag.create_int_list([
        (most >> 32) as i32,
        most as i32,
        (least >> 32) as i32,
        least as i32,
    ])
}
