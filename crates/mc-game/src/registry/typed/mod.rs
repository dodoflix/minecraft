//! Typed values for the registries this slice owns (`dimension_type`, `worldgen/biome`, `damage_type`, `chat_type`,
//! `dimension` (level stems) and the `noise` / `density_function` / `noise_settings` worldgen registries). Other registries
//! register their own decoder through [`super::decoders::DYNAMIC_REGISTRY_DECODERS`].

pub mod attributes;
pub mod biome;
pub mod chat_type;
pub mod damage_type;
pub mod dialog;
pub mod dimension_type;
pub mod int_provider;
pub mod level_stem;
pub mod network;
pub mod worldgen;

use serde_json::Value;

/// `HolderSet<E>` as decoded: a `#tag` reference or a direct list of element ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HolderSetRef {
    Tag(mc_core::ident::Identifier),
    Elements(Vec<mc_core::ident::Identifier>),
}

impl HolderSetRef {
    /// Read validated holder-set JSON (`"#tag"`, `"id"` or `["id", ...]`).
    pub fn from_json(v: &Value) -> Option<HolderSetRef> {
        use mc_core::ident::Identifier;
        match v {
            Value::String(s) => match s.strip_prefix('#') {
                Some(tag) => Identifier::try_parse(tag).map(HolderSetRef::Tag),
                None => Identifier::try_parse(s).map(|i| HolderSetRef::Elements(vec![i])),
            },
            Value::Array(list) => list.iter().map(|e| e.as_str().and_then(Identifier::try_parse)).collect::<Option<Vec<_>>>().map(HolderSetRef::Elements),
            _ => None,
        }
    }

    /// The element ids of this set in `registry` (`HolderSet.stream()`), resolving tags through the registry.
    pub fn resolve(&self, registry: &crate::registry::Registry) -> Vec<u32> {
        match self {
            HolderSetRef::Tag(tag) => registry.tag(tag).map(<[u32]>::to_vec).unwrap_or_default(),
            HolderSetRef::Elements(ids) => ids.iter().filter_map(|i| registry.id_of(i)).collect(),
        }
    }
}

pub(crate) fn get_bool(o: &Value, key: &str) -> Option<bool> {
    match o.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => Some(n.as_i64().unwrap_or(0) as i8 != 0),
        _ => None,
    }
}

pub(crate) fn get_i32(o: &Value, key: &str) -> Option<i32> {
    let v = o.get(key)?;
    v.as_i64().map(|i| i as i32).or_else(|| v.as_f64().map(|f| f as i32))
}

pub(crate) fn get_f64(o: &Value, key: &str) -> Option<f64> {
    o.get(key)?.as_f64()
}

pub(crate) fn get_f32(o: &Value, key: &str) -> Option<f32> {
    get_f64(o, key).map(|d| d as f32)
}

pub(crate) fn get_str<'a>(o: &'a Value, key: &str) -> Option<&'a str> {
    o.get(key)?.as_str()
}
