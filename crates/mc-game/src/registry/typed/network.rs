//! Network-codec schemas for synchronized registries that have no typed Rust value yet: they validate the entry and produce
//! the exact `registry_data` NBT (numeric tag types, dropped server-only fields such as `spawn_conditions`) without a decoder.
//! A registry with a typed decoder (`DYNAMIC_REGISTRY_DECODERS`) takes precedence.

use serde_json::Value;

use crate::registry::codec::{Field, Schema, default, opt, req};
use crate::registry::keys;

fn sound_event() -> Schema {
    Schema::custom(|v, ctx| match v {
        Value::String(_) => Schema::StaticRegistry(keys::SOUND_EVENT).convert(v, ctx),
        _ => Schema::Record(vec![req("sound_id", Schema::Identifier), crate::registry::codec::lenient("range", Schema::Float)]).convert(v, ctx),
    })
}

/// `ExtraCodecs.POSITIVE_FLOAT`
fn positive_float() -> Schema {
    Schema::custom(|v, ctx| {
        let t = Schema::Float.convert(v, ctx)?;
        match t {
            mc_nbt::Tag::Float(f) if f > 0.0 => Ok(mc_nbt::Tag::Float(f)),
            mc_nbt::Tag::Float(f) => Err(format!("Value must be positive: {}", mc_core::math::java::java_float_to_string(f))),
            other => Ok(other),
        }
    })
}

fn non_negative_float() -> Schema {
    Schema::custom(|v, ctx| match Schema::Float.convert(v, ctx)? {
        mc_nbt::Tag::Float(f) if f < 0.0 => Err(format!("Value must be non-negative: {}", mc_core::math::java::java_float_to_string(f))),
        other => Ok(other),
    })
}

/// `ModelAndTexture.codec(...)` + `baby_asset_id` (pig, cow, chicken); the asset ids are plain strings in the data.
fn animal_variant(baby: bool) -> Vec<Field> {
    let mut f = vec![opt("asset_id", Schema::Raw), opt("model", Schema::Raw)];
    if baby {
        f.push(req("baby_asset_id", Schema::Raw));
    }
    f
}

/// The `NETWORK_CODEC` (or `DIRECT_CODEC` when identical) of `registry`, if modelled.
pub fn network_schema(registry: &str) -> Option<Schema> {
    Some(match registry {
        keys::PIG_VARIANT | keys::COW_VARIANT | keys::CHICKEN_VARIANT => Schema::Record(animal_variant(true)),
        keys::ZOMBIE_NAUTILUS_VARIANT => Schema::Record(animal_variant(false)),
        keys::CAT_VARIANT => Schema::Record(vec![opt("asset_id", Schema::Raw), req("baby_asset_id", Schema::Raw)]),
        keys::FROG_VARIANT => Schema::Record(vec![opt("asset_id", Schema::Raw)]),
        keys::WOLF_VARIANT => Schema::Record(vec![req("assets", Schema::Raw), req("baby_assets", Schema::Raw)]),
        keys::PAINTING_VARIANT => Schema::Record(vec![
            req("width", Schema::IntRange(1, 16)),
            req("height", Schema::IntRange(1, 16)),
            req("asset_id", Schema::Identifier),
            opt("title", Schema::Text),
            opt("author", Schema::Text),
        ]),
        keys::JUKEBOX_SONG => Schema::Record(vec![
            req("sound_event", sound_event()),
            req("description", Schema::Text),
            req("length_in_seconds", positive_float()),
            req("comparator_output", Schema::IntRange(0, 15)),
        ]),
        keys::INSTRUMENT => Schema::Record(vec![
            req("sound_event", sound_event()),
            req("use_duration", non_negative_float()),
            req("range", positive_float()),
            default("durability_damage", Schema::IntRange(0, i32::MAX), Value::from(0)),
            req("description", Schema::Text),
        ]),
        _ => return None,
    })
}
