//! `Dynamic<Tag>` read semantics (`Dynamic.get/asInt/asBoolean/asStream/...` over `NbtOps`) and Java
//! `HashSet` iteration order, which vanilla's list-from-set encoders expose in the written files.

use mc_core::math::java::java_string_hash;
use mc_nbt::{CompoundTag, ListTag, Tag};

/// `Dynamic.get(key)`: only compounds have keys; anything else behaves like an absent field.
pub(crate) fn get<'a>(tag: Option<&'a Tag>, key: &str) -> Option<&'a Tag> {
    tag?.as_compound()?.get(key)
}

/// `Dynamic.asInt(default)`: any numeric tag via `Number.intValue()`.
pub(crate) fn as_int(tag: Option<&Tag>, default: i32) -> i32 {
    tag.and_then(Tag::as_int).unwrap_or(default)
}

/// `Dynamic.asLong(default)`.
pub(crate) fn as_long(tag: Option<&Tag>, default: i64) -> i64 {
    tag.and_then(Tag::as_long).unwrap_or(default)
}

/// `Dynamic.asBoolean(default)`: `NbtOps.getBooleanValue` is `doubleValue() != 0.0`.
pub(crate) fn as_boolean(tag: Option<&Tag>, default: bool) -> bool {
    tag.and_then(Tag::as_number).map(|n| n.double_value() != 0.0).unwrap_or(default)
}

/// `Dynamic.asString(default)`: only string tags.
pub(crate) fn as_string(tag: Option<&Tag>, default: &str) -> String {
    tag.and_then(Tag::as_string).unwrap_or(default).to_owned()
}

/// `NbtOps.getStream`: the elements of any `CollectionTag` (list or primitive array); `None` otherwise.
pub(crate) fn stream(tag: &Tag) -> Option<Vec<Tag>> {
    Some(match tag {
        Tag::List(l) => l.to_vec(),
        Tag::ByteArray(v) => v.iter().map(|&b| Tag::Byte(b)).collect(),
        Tag::IntArray(v) => v.iter().map(|&i| Tag::Int(i)).collect(),
        Tag::LongArray(v) => v.iter().map(|&l| Tag::Long(l)).collect(),
        _ => return None,
    })
}

/// `Codec.STRING.listOf()` decode: a collection of strings, any other element makes the whole decode fail.
pub(crate) fn string_list(tag: &Tag) -> Option<Vec<String>> {
    stream(tag)?.iter().map(|t| t.as_string().map(str::to_owned)).collect()
}

/// `dynamic.asStream().flatMap(e -> e.asString().result().stream())`: the string elements, others skipped.
pub(crate) fn lenient_strings(tag: Option<&Tag>) -> Vec<String> {
    tag.and_then(stream)
        .map(|v| v.iter().filter_map(|t| t.as_string().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// `Codec.INT_STREAM` + `Util.fixedSize(n)`: an int array, or a collection of numbers (each via `intValue`).
pub(crate) fn fixed_int_stream(tag: &Tag, size: usize) -> Result<Vec<i32>, String> {
    let ints: Vec<i32> = match tag {
        Tag::IntArray(v) => v.clone(),
        other => stream(other)
            .ok_or_else(|| "Not a list".to_owned())?
            .iter()
            .map(|t| t.as_int().ok_or_else(|| "Not a number".to_owned()))
            .collect::<Result<_, _>>()?,
    };
    if ints.len() == size {
        Ok(ints)
    } else {
        Err(format!("Input is not a list of {size} ints"))
    }
}

pub(crate) fn string_list_tag<'a>(values: impl IntoIterator<Item = &'a String>) -> ListTag {
    let mut list = ListTag::new();
    for v in values {
        list.push(Tag::String(v.clone()));
    }
    list
}

/// Iteration order of a Java `HashSet<E>` (default capacity) built by inserting `items` in order
/// (`mc_core::java_collections::hash_map_order`). Duplicates collapse.
pub(crate) fn java_hash_set_order<T: PartialEq>(items: Vec<(i32, T)>) -> Vec<T> {
    let mut unique: Vec<(i32, T)> = Vec::with_capacity(items.len());
    for (hash, item) in items {
        if !unique.iter().any(|(_, existing)| *existing == item) {
            unique.push((hash, item));
        }
    }
    mc_core::java_collections::hash_map_ordered(unique, |e| e.0, mc_core::java_collections::HASH_MAP_DEFAULT_CAPACITY)
        .into_iter()
        .map(|(_, item)| item)
        .collect()
}

/// [`java_hash_set_order`] for `HashSet<String>`.
pub(crate) fn java_string_set_order(items: &[String]) -> Vec<String> {
    java_hash_set_order(items.iter().map(|s| (java_string_hash(s), s.clone())).collect())
}

/// Copy of `tag` without the keys listed in `known` (what vanilla's parse does not consume).
pub(crate) fn without_keys(tag: &CompoundTag, known: &[&str]) -> CompoundTag {
    tag.iter().filter(|(k, _)| !known.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_set_order_matches_java() {
        // new HashSet<>(List.of("b","a","c")) iterates a, b, c ("a".hashCode() == 97 → bucket 1, ...)
        let order = java_string_set_order(&["c".into(), "b".into(), "a".into(), "a".into()]);
        assert_eq!(order, ["a", "b", "c"]);
    }
}
