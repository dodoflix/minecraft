//! Network-codec schemas for synchronized registries that have no typed Rust value yet: they validate the entry and produce
//! the exact `registry_data` NBT (numeric tag types, dropped server-only fields such as `spawn_conditions`) without a decoder.
//! A registry with a typed decoder (`DYNAMIC_REGISTRY_DECODERS`) takes precedence.

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};
use serde_json::Value;

use crate::registry::codec::{Field, Schema, default, int_value, opt, req};
use crate::registry::keys;

pub(super) fn sound_event() -> Schema {
    Schema::custom(|v, ctx| match v {
        Value::String(_) => Schema::StaticRegistry(keys::SOUND_EVENT).convert(v, ctx),
        _ => Schema::Record(vec![
            req("sound_id", Schema::Identifier),
            crate::registry::codec::lenient("range", Schema::Float),
        ])
        .convert(v, ctx),
    })
}

/// `ExtraCodecs.POSITIVE_FLOAT`
pub(super) fn positive_float() -> Schema {
    Schema::custom(|v, ctx| {
        let t = Schema::Float.convert(v, ctx)?;
        match t {
            // `Float.compareTo` ordering: `0.0` and `Float.MAX_VALUE` bound the range
            mc_nbt::Tag::Float(f)
                if f.total_cmp(&0.0).is_gt() && f.total_cmp(&f32::MAX).is_le() =>
            {
                Ok(mc_nbt::Tag::Float(f))
            }
            mc_nbt::Tag::Float(f) => Err(format!(
                "Value must be positive: {}",
                mc_core::math::java::java_float_to_string(f)
            )),
            other => Ok(other),
        }
    })
}

/// `FloatProviders.CODEC` as the network form of a value that was already validated: a constant is the bare float,
/// the other types are their record with the `type` id.
fn float_provider() -> Schema {
    Schema::custom(|v, ctx| {
        if v.is_number() {
            return Schema::Float.convert(v, ctx);
        }
        let body = Schema::Dispatch {
            key: "type",
            resolve: |ty| {
                let float = |name| req(name, Schema::Float);
                Some(Schema::Record(
                    match ty.strip_prefix("minecraft:").unwrap_or(ty) {
                        "constant" => vec![float("value")],
                        "uniform" => vec![float("min_inclusive"), float("max_exclusive")],
                        "clamped_normal" => {
                            vec![
                                float("mean"),
                                float("deviation"),
                                float("min"),
                                float("max"),
                            ]
                        }
                        "trapezoid" => vec![float("min"), float("max"), float("plateau")],
                        _ => return None,
                    },
                ))
            },
        }
        .convert(v, ctx)?;
        match (&body, v.get("type").and_then(Value::as_str)) {
            (mc_nbt::Tag::Compound(c), Some(ty))
                if ty.strip_prefix("minecraft:").unwrap_or(ty) == "constant" =>
            {
                Ok(c.get("value").cloned().unwrap_or(body.clone()))
            }
            _ => Ok(body),
        }
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
        keys::BLOCK_STATE_PROVIDER => super::state_provider::direct_schema(),
        keys::BLOCK_TRANSFORMER => super::state_provider::block_transformer_schema(),
        keys::PIG_VARIANT | keys::COW_VARIANT | keys::CHICKEN_VARIANT => {
            Schema::Record(animal_variant(true))
        }
        keys::ZOMBIE_NAUTILUS_VARIANT => Schema::Record(animal_variant(false)),
        keys::CAT_VARIANT => Schema::Record(vec![
            opt("asset_id", Schema::Raw),
            req("baby_asset_id", Schema::Raw),
        ]),
        keys::FROG_VARIANT => Schema::Record(vec![opt("asset_id", Schema::Raw)]),
        keys::WOLF_VARIANT => Schema::Record(vec![
            req("assets", Schema::Raw),
            req("baby_assets", Schema::Raw),
        ]),
        keys::PAINTING_VARIANT => Schema::Record(vec![
            req("width", Schema::ExtraIntRange(1, 16)),
            req("height", Schema::ExtraIntRange(1, 16)),
            req("asset_id", Schema::Identifier),
            opt("title", Schema::Text),
            opt("author", Schema::Text),
        ]),
        keys::SULFUR_CUBE_ARCHETYPE => Schema::Record(vec![
            req(
                "items",
                Schema::HolderSet {
                    registry: keys::ITEM,
                    always_list: false,
                },
            ),
            req(
                "attribute_modifiers",
                Schema::list_of(Schema::Record(vec![
                    // the typed decoder already resolved the attribute
                    req("attribute", Schema::Identifier),
                    req("id", Schema::Identifier),
                    req("amount", Schema::Double),
                    req(
                        "operation",
                        Schema::Enum(&["add_value", "add_multiplied_base", "add_multiplied_total"]),
                    ),
                ])),
            ),
            default("buoyant", Schema::Bool, Value::from(false)),
            opt(
                "explosion",
                Schema::Record(vec![
                    req("power", Schema::ExtraIntRange(0, i32::MAX)),
                    req("causes_fire", Schema::Bool),
                    req("fuse", Schema::ExtraIntRange(1, i32::MAX)),
                ]),
            ),
            opt(
                "contact_damage",
                Schema::Record(vec![
                    req("damage_type", Schema::Holder(keys::DAMAGE_TYPE)),
                    req("amount", float_provider()),
                    req("attribute_to_source", Schema::Bool),
                ]),
            ),
            req(
                "knockback_modifiers",
                Schema::Record(vec![
                    req("horizontal_power", Schema::Float),
                    req("vertical_power", Schema::Float),
                ]),
            ),
            req(
                "sound_settings",
                Schema::Record(vec![
                    req("hit_sound", sound_event()),
                    req("push_sound", sound_event()),
                    req("push_sound_impulse_threshold", Schema::Float),
                    req("push_sound_cooldown", Schema::Float),
                ]),
            ),
        ]),
        keys::TIMELINE => timeline(),
        _ => return None,
    })
}

/// `Timeline.NETWORK_CODEC`: `DIRECT_CODEC` (`period_ticks` bounds the markers and keyframes) without the tracks of
/// attributes that are not synced. Empty `tracks` and `time_markers` are omitted, a marker that is not shown in
/// commands is its bare ticks.
fn timeline() -> Schema {
    use super::attributes::{attribute, non_negative_int, track_schema};
    let marker = Schema::custom(|v, ctx| {
        let full = Schema::Record(vec![
            req("ticks", non_negative_int()),
            default("show_in_commands", Schema::Bool, Value::from(false)),
        ]);
        match Schema::either(non_negative_int(), full).convert(v, ctx)? {
            Tag::Compound(c) if c.get("show_in_commands").is_none() => {
                Ok(c.get("ticks").cloned().unwrap_or(Tag::Compound(c)))
            }
            other => Ok(other),
        }
    });
    let tracks = Schema::custom(|v, ctx| {
        let object = v.as_object().ok_or_else(|| ctx.not_a_map(v))?;
        let mut out = CompoundTag::new();
        for (key, value) in object {
            let id = Identifier::read(key)?;
            let def = attribute(&id).ok_or_else(|| {
                format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:environment_attribute]: ResourceKey[minecraft:environment_attribute / {id}]")
            })?;
            let track = track_schema(def).convert(value, ctx)?;
            if def.syncable {
                out.put(id.to_string(), track);
            }
        }
        Ok(Tag::Compound(out))
    });
    let record = Schema::Record(vec![
        req("clock", Schema::HolderFixed(keys::WORLD_CLOCK)),
        opt("period_ticks", Schema::ExtraIntRange(1, i32::MAX)),
        default("tracks", tracks, serde_json::json!({})),
        default(
            "time_markers",
            Schema::map_of(Schema::Identifier, marker),
            serde_json::json!({}),
        ),
    ]);
    Schema::custom(move |v, ctx| {
        let tag = record.convert(v, ctx)?;
        validate_period(v)?;
        Ok(tag)
    })
}

/// `Timeline.validateInternal` over a timeline that decoded: with a `period_ticks`, every time marker lies in
/// `[0; period)` and every keyframe tick in `[0; period]`.
fn validate_period(v: &Value) -> Result<(), String> {
    // an int codec truncates a fractional number
    let int = |v: Option<&Value>| v.and_then(Value::as_number).map_or(0, int_value);
    let Some(period) = v
        .get("period_ticks")
        .and_then(Value::as_number)
        .map(int_value)
    else {
        return Ok(());
    };
    let markers = v.get("time_markers").and_then(Value::as_object);
    for (key, marker) in markers.into_iter().flatten() {
        let ticks = int(Some(marker.get("ticks").unwrap_or(marker)));
        if ticks < 0 || ticks >= period {
            let id = Identifier::read(key)?;
            return Err(format!(
                "Time Marker ResourceKey[minecraft:clock_time_marker / {id}] must be in range [0; {period})"
            ));
        }
    }
    let tracks = v.get("tracks").and_then(Value::as_object);
    for track in tracks.into_iter().flatten().map(|(_, t)| t) {
        let keyframes = track.get("keyframes").and_then(Value::as_array);
        for keyframe in keyframes.into_iter().flatten() {
            let ticks = int(keyframe.get("ticks"));
            if ticks < 0 || ticks > period {
                return Err(format!(
                    "Keyframe at tick {ticks} must be in range [0; {period}]"
                ));
            }
        }
    }
    Ok(())
}
