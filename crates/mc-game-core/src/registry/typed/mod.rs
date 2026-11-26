//! Typed values for the registries this module owns (`dimension_type`, `worldgen/biome`, `damage_type`, `chat_type`,
//! `dimension` (level stems) and the `noise` / `density_function` / `noise_settings` worldgen registries). Other registries
//! register their own decoder through [`super::decoders::DYNAMIC_REGISTRY_DECODERS`].

pub mod attributes;
pub mod biome;
pub mod chat_type;
pub mod damage_type;
pub mod dialog;
pub mod dimension_type;
pub mod enchantment;
pub mod int_provider;
pub mod level_stem;
pub mod network;
pub mod noise_settings;
pub mod painting_variant;
pub mod state_provider;
pub mod worldgen;

use mc_dfu::JNum;
use serde_json::Value;

use super::codec::{RegistryDecodeContext, jnum};

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
            Value::Array(list) => list
                .iter()
                .map(|e| e.as_str().and_then(Identifier::try_parse))
                .collect::<Option<Vec<_>>>()
                .map(HolderSetRef::Elements),
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

/// The number at `key` as the codec reads it (also one beyond `f64`, which `serde_json` holds as `null`).
fn get_number(o: &Value, key: &str, ctx: &RegistryDecodeContext) -> Option<JNum> {
    let v = o.get(key)?;
    match v {
        Value::Number(n) => Some(jnum(n)),
        _ => ctx.lazy_number(v).map(|t| JNum::Lazy(t.to_owned())),
    }
}

pub(crate) fn get_i32(o: &Value, key: &str, ctx: &RegistryDecodeContext) -> Option<i32> {
    get_number(o, key, ctx).map(|n| n.int_value())
}

pub(crate) fn get_f64(o: &Value, key: &str, ctx: &RegistryDecodeContext) -> Option<f64> {
    get_number(o, key, ctx).map(|n| n.double_value())
}

pub(crate) fn get_f32(o: &Value, key: &str, ctx: &RegistryDecodeContext) -> Option<f32> {
    get_number(o, key, ctx).map(|n| n.float_value())
}

pub(crate) fn get_str<'a>(o: &'a Value, key: &str) -> Option<&'a str> {
    o.get(key)?.as_str()
}
