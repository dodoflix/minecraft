//! `BlockStateProvider.DIRECT_CODEC` and `BlockTransformer.DIRECT_CODEC` as [`Schema`]s: they validate an entry like the
//! codecs do and produce what `encodeStart(NbtOps, ..)` writes (block states with their properties in codec order, typed
//! numbers, defaults omitted). The `block_state_provider` and `block_transformer` registries, the providers inside
//! enchantment effects and the transformers share them.

use mc_nbt::Tag;
use serde_json::Value;

use super::int_provider;
use super::network::{positive_float, sound_event};
use crate::registry::codec::{Field, RegistryDecodeContext, Schema, default, opt, req};
use crate::registry::keys;
use crate::world::item::enchantment::value_codecs::{
    block_predicate_schema, block_state_schema, block_state_tag, parse_block_state_json,
    parse_full_state,
};

const DIRECTIONS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// `BlockStateProvider.CODEC`: a registry reference or an inline provider.
fn holder() -> Schema {
    Schema::HolderOrDirect {
        registry: keys::BLOCK_STATE_PROVIDER,
        direct,
    }
}

/// `BlockStateProvider.DIRECT_CODEC`.
pub fn direct_schema() -> Schema {
    Schema::custom(direct)
}

/// `BlockStateProvider.DIRECT_CODEC`: `Codec.xor(BlockState.FULL_CODEC, TYPED_CODEC)`, a `simple` provider re-encoding as
/// its state.
pub fn direct(v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, String> {
    match (full_state(v), typed(v, ctx)) {
        (Ok(_), Ok(_)) => {
            Err("Both alternatives read successfully, can not pick the correct one".to_owned())
        }
        (Ok(tag), _) | (_, Ok(tag)) => Ok(tag),
        // vanilla lists the typed alternative's error first
        (Err(full), Err(typed)) => Err(format!("{typed}; {full}")),
    }
}

/// `BlockState.FULL_CODEC`: `id` is required, even for a block without properties.
fn full_state(v: &Value) -> Result<Tag, String> {
    parse_full_state(v).map(|state| block_state_tag(state, false))
}

/// `TYPED_CODEC`: the `type` key selects the provider's map codec.
fn typed(v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, String> {
    if !v.is_object() {
        return Err(format!("Not a JSON object: {v}"));
    }
    let id = v
        .get("type")
        .and_then(Value::as_str)
        .and_then(|t| mc_core::ident::Identifier::read(t).ok())
        .map(|id| id.to_string());
    if let Some(id) = id.as_deref()
        && provider_type(id).is_none()
    {
        return Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:worldgen/block_state_provider_type]: {id}"
        ));
    }
    let tag = Schema::Dispatch {
        key: "type",
        resolve: provider_type,
    }
    .convert(v, ctx)?;
    if id.as_deref() != Some("minecraft:simple") {
        return Ok(tag);
    }
    match v.get("state").map(parse_block_state_json) {
        Some(Ok(state)) => Ok(block_state_tag(state, false)),
        _ => Ok(tag),
    }
}

fn provider_type(name: &str) -> Option<Schema> {
    let states = || Schema::NonEmptyList(Box::new(block_state_schema()));
    Some(match name {
        "minecraft:copy_properties" => Schema::Record(vec![req("source", holder())]),
        "minecraft:dual_noise" => {
            let mut fields = vec![
                req("variety", variety()),
                req("slow_noise", noise_parameters()),
                req("slow_scale", positive_float()),
            ];
            fields.extend(noise_fields());
            fields.push(req("states", states()));
            Schema::Record(fields)
        }
        "minecraft:noise" => {
            let mut fields = noise_fields();
            fields.push(req("states", states()));
            Schema::Record(fields)
        }
        "minecraft:noise_threshold" => {
            let mut fields = noise_fields();
            fields.extend([
                req("threshold", Schema::FloatRange(-1.0, 1.0)),
                req("high_chance", Schema::FloatRange(0.0, 1.0)),
                req("default_state", block_state_schema()),
                req("low_states", states()),
                req("high_states", states()),
            ]);
            Schema::Record(fields)
        }
        "minecraft:random_block" => Schema::Record(vec![req(
            "blocks",
            Schema::HolderSet {
                registry: keys::BLOCK,
                always_list: false,
            },
        )]),
        "minecraft:randomized_int" => Schema::Record(vec![
            req("source", holder()),
            req("property", Schema::String),
            req("values", int_provider::schema(i32::MIN, i32::MAX)),
        ]),
        "minecraft:rotated" => Schema::Record(vec![
            req("state", holder()),
            opt("direction", Schema::Enum(&DIRECTIONS)),
        ]),
        "minecraft:rule_based" => Schema::Record(vec![
            opt("fallback", holder()),
            req(
                "rules",
                Schema::list_of(Schema::Record(vec![
                    req("if_true", block_predicate_schema()),
                    req("then", holder()),
                ])),
            ),
        ]),
        "minecraft:simple" => Schema::Record(vec![req("state", block_state_schema())]),
        "minecraft:weighted" => Schema::Record(vec![req("entries", weighted_states())]),
        _ => return None,
    })
}

/// `NoiseBasedStateProvider.noiseCodec`.
fn noise_fields() -> Vec<Field> {
    vec![
        req("seed", Schema::Long),
        req("noise", noise_parameters()),
        req("scale", positive_float()),
    ]
}

/// `ExtraCodecs.NON_NEGATIVE_INT`.
fn non_negative_int() -> Schema {
    Schema::custom(|v, ctx| match Schema::Int.convert(v, ctx)? {
        Tag::Int(n) if n < 0 => Err(format!("Value must be non-negative: {n}")),
        other => Ok(other),
    })
}

/// `Codec.listOf(min, max)`.
fn list_between(item: Schema, min: usize, max: usize) -> Schema {
    Schema::custom(move |v, ctx| {
        let tag = Schema::list_of(item.clone()).convert(v, ctx)?;
        let size = match &tag {
            Tag::List(list) => list.len(),
            _ => 0,
        };
        if size < min {
            Err(format!(
                "List is too short: {size}, expected range [{min}-{max}]"
            ))
        } else if size > max {
            Err(format!(
                "List is too long: {size}, expected range [{min}-{max}]"
            ))
        } else {
            Ok(tag)
        }
    })
}

/// `WeightedList.nonEmptyCodec(BlockState.CODEC)`: `{data, weight}` entries, at least one with weight.
fn weighted_states() -> Schema {
    Schema::custom(|v, ctx| {
        let entry = Schema::Record(vec![
            req("data", block_state_schema()),
            req("weight", non_negative_int()),
        ]);
        let tag = Schema::list_of(entry).convert(v, ctx)?;
        let total = match &tag {
            Tag::List(list) => list.iter().fold(0i32, |sum, e| {
                sum.wrapping_add(
                    e.as_compound()
                        .and_then(|c| c.get_int("weight"))
                        .unwrap_or(0),
                )
            }),
            _ => 0,
        };
        if total == 0 {
            return Err(
                "Weighted list must contain at least one entry with non-zero weight".to_owned(),
            );
        }
        Ok(tag)
    })
}

/// `InclusiveRange.codec(Codec.INT, 1, 64)`: a number, a `[min, max]` list or a `{min_inclusive, max_inclusive}` object;
/// equal bounds encode as the single number, else as a list.
fn variety() -> Schema {
    fn int(v: &Value, ctx: &RegistryDecodeContext) -> Result<i32, String> {
        match Schema::Int.convert(v, ctx)? {
            Tag::Int(n) => Ok(n),
            other => Err(format!("Not a number: {other:?}")),
        }
    }
    Schema::custom(|v, ctx| {
        let (min, max) = match v {
            Value::Number(_) => {
                let n = int(v, ctx)?;
                (n, n)
            }
            Value::Array(list) => {
                let [min, max] = list.as_slice() else {
                    return Err("Input is not a list of 2 elements".to_owned());
                };
                (int(min, ctx)?, int(max, ctx)?)
            }
            Value::Object(o) => {
                let bound = |key: &str| {
                    o.get(key)
                        .ok_or_else(|| format!("No key {key} in MapLike[{v}]"))
                        .and_then(|b| int(b, ctx))
                };
                (bound("min_inclusive")?, bound("max_inclusive")?)
            }
            _ => return Err(format!("Not a number: {v}")),
        };
        if min > max {
            return Err("min_inclusive must be less than or equal to max_inclusive".to_owned());
        }
        if min < 1 {
            return Err(format!(
                "Range limit too low, expected at least 1 [{min}-{max}]"
            ));
        }
        if max > 64 {
            return Err(format!(
                "Range limit too high, expected at most 64 [{min}-{max}]"
            ));
        }
        Ok(if min == max {
            Tag::Int(min)
        } else {
            Tag::List(
                [Tag::Int(min), Tag::Int(max)]
                    .into_iter()
                    .collect::<mc_nbt::ListTag>(),
            )
        })
    })
}

/// `NormalNoise.Normalization.CODEC`: a boolean or the string `legacy`.
fn normalization() -> Schema {
    Schema::custom(|v, ctx| match v {
        Value::String(s) if s == "legacy" => Ok(Tag::String(s.clone())),
        Value::String(s) => Err(format!("Invalid normalization type: {s}")),
        _ => Schema::Bool.convert(v, ctx),
    })
}

/// `NormalNoise.Parameters.CODEC` (with its `validate`).
fn noise_parameters() -> Schema {
    let record = Schema::Record(vec![
        default(
            "base_amplitude",
            Schema::DoubleRange(f64::from(1.0E-5f32), 1_000_000.0),
            Value::from(1.0),
        ),
        req("base_octave", Schema::IntRange(-32, 32)),
        default("octave_count", Schema::IntRange(1, 32), Value::from(1)),
        default("normalize", normalization(), Value::Bool(true)),
        default(
            "amplitude_modifiers",
            list_between(Schema::DoubleRange(0.0, 1_000_000.0), 0, 32),
            Value::Array(Vec::new()),
        ),
    ]);
    Schema::custom(move |v, ctx| {
        let tag = record.convert(v, ctx)?;
        if let Tag::Compound(c) = &tag {
            let modifiers = c.get_list("amplitude_modifiers").map_or(0, |l| l.len());
            let octaves = c.get_int("octave_count").unwrap_or(1);
            if modifiers != 0 && modifiers as i32 != octaves {
                return Err(format!(
                    "amplitude_modifiers had size {modifiers}, but octave_count was {octaves}"
                ));
            }
        }
        Ok(tag)
    })
}

/// `BlockTransformer.DIRECT_CODEC`: one to 200 transforms.
pub fn block_transformer_schema() -> Schema {
    let transform = Schema::Record(vec![
        req("block_state_provider", holder()),
        default(
            "sound",
            sound_event(),
            Value::from("minecraft:intentionally_empty"),
        ),
        default(
            "particle",
            Schema::Enum(&["none", "scrape", "wax_on", "wax_off"]),
            Value::from("none"),
        ),
        default(
            "disallowed_faces",
            Schema::list_of(Schema::Enum(&DIRECTIONS)),
            Value::Array(Vec::new()),
        ),
        opt("loot", Schema::Identifier),
        default(
            "drop_strategy",
            Schema::Enum(&["clicked_face", "from_middle"]),
            Value::from("from_middle"),
        ),
        default("update_from_neighbors", Schema::Bool, Value::Bool(true)),
        default(
            "transform_type",
            Schema::Enum(&["single_block", "copper_chest"]),
            Value::from("single_block"),
        ),
        default("consume_on_use", Schema::Bool, Value::Bool(true)),
        default("item_damage_per_use", non_negative_int(), Value::from(0)),
    ]);
    list_between(transform, 1, 200)
}
