//! `BlockStateProvider.DIRECT_CODEC` and `BlockTransformer.DIRECT_CODEC` as [`Schema`]s: they validate an entry like the
//! codecs do and produce what `encodeStart(NbtOps, ..)` writes (block states with their properties in codec order, typed
//! numbers, defaults omitted). The `block_state_provider` and `block_transformer` registries, the providers inside
//! enchantment effects and the transformers share them.

use mc_nbt::Tag;
use serde_json::Value;

use super::int_provider;
use super::network::{positive_float, sound_event};
use crate::registry::codec::{
    Failure, Field, RegistryDecodeContext, Schema, default, illegal_argument, int_value,
    interval_codec, opt, req, validate, xor,
};
use crate::registry::keys;
use crate::world::item::enchantment::value_codecs::{
    block_predicate_schema, block_state_schema, block_state_tag, parse_block_state_json,
    parse_full_state, vertical_anchor_text,
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
    Schema::custom_partial(direct)
}

/// `BlockStateProvider.DIRECT_CODEC`: `Codec.xor(BlockState.FULL_CODEC, TYPED_CODEC)`, a `simple` provider re-encoding as
/// its state.
pub fn direct(v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, Failure> {
    xor(full_state(v), typed(v, ctx), |_, _| {
        both_alternatives(v, ctx)
    })
}

/// `BlockState.FULL_CODEC`: `id` is required, even for a block without properties.
fn full_state(v: &Value) -> Result<Tag, Failure> {
    parse_full_state(v)
        .map(|state| block_state_tag(state, false))
        .map_err(Failure::from)
}

/// `TYPED_CODEC`: the `type` key selects the provider's map codec.
fn typed(v: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, Failure> {
    let tag =
        Schema::dispatch("worldgen/block_state_provider_type", provider_type).decode(v, ctx)?;
    if provider_type_id(v).as_deref() != Some("minecraft:simple") {
        return Ok(tag);
    }
    match v.get("state").map(parse_block_state_json) {
        Some(Ok(state)) => Ok(block_state_tag(state, false)),
        _ => Ok(tag),
    }
}

/// The `type` of a provider as the registry key (`byNameCodec`, an unprefixed name is in `minecraft`).
fn provider_type_id(v: &Value) -> Option<String> {
    v.get("type")
        .and_then(Value::as_str)
        .and_then(|t| mc_core::ident::Identifier::read(t).ok())
        .map(|id| id.to_string())
}

/// `XorCodec`'s error when both alternatives read: it prints both results (`Pair.toString`: the value and the input).
fn both_alternatives(v: &Value, ctx: &RegistryDecodeContext) -> String {
    let state = parse_full_state(v)
        .map(|s| s.to_string())
        .unwrap_or_default();
    format!(
        "Both alternatives read successfully, can not pick the correct one; first: (Left[{state}], {v}) second: (Right[{}], {v})",
        provider_string(v, ctx)
    )
}

/// Java's `toString` of the provider an (already validated) typed provider JSON decodes to. Classes that are not records
/// print their identity hash code (`Class@1b2c3d`), which differs between runs of the vanilla server: it is written as
/// `@0` and comparisons mask the hash. Registry references of the registry being loaded are not bound yet, so they print
/// without their value.
fn provider_string(v: &Value, ctx: &RegistryDecodeContext) -> String {
    const PACKAGE: &str = "net.minecraft.world.level.levelgen.feature.stateproviders";
    let inline = |v: &Value| match parse_full_state(v) {
        Ok(state) => format!("Direct{{SimpleStateProvider[state={state}]}}"),
        Err(_) => format!("Direct{{{}}}", provider_string(v, ctx)),
    };
    let holder = |v: Option<&Value>| match v {
        Some(Value::String(id)) => {
            let id = mc_core::ident::Identifier::read(id)
                .map_or_else(|_| id.clone(), |id| id.to_string());
            format!("Reference{{ResourceKey[minecraft:worldgen/block_state_provider / {id}]=null}}")
        }
        Some(v) => inline(v),
        None => "null".to_owned(),
    };
    match provider_type_id(v).as_deref() {
        Some("minecraft:simple") => {
            let state = v.get("state").map(parse_block_state_json);
            let state = state.and_then(Result::ok).map(|s| s.to_string());
            format!("SimpleStateProvider[state={}]", state.unwrap_or_default())
        }
        Some("minecraft:random_block") => {
            let blocks = match v.get("blocks") {
                Some(Value::String(tag)) if tag.starts_with('#') => {
                    let id = mc_core::ident::Identifier::read(&tag[1..])
                        .map_or_else(|_| tag[1..].to_owned(), |id| id.to_string());
                    format!("NamedSet(TagKey[minecraft:block / {id}])[null]")
                }
                blocks => {
                    let blocks = match blocks {
                        Some(Value::Array(list)) => list.iter().collect(),
                        Some(single) => vec![single],
                        None => Vec::new(),
                    };
                    let blocks: Vec<String> = blocks
                        .into_iter()
                        .filter_map(Value::as_str)
                        .filter_map(|id| mc_core::ident::Identifier::read(id).ok())
                        .map(|id| {
                            format!(
                                "Reference{{ResourceKey[minecraft:block / {id}]=Block{{{id}}}}}"
                            )
                        })
                        .collect();
                    format!("DirectSet[[{}]]", blocks.join(", "))
                }
            };
            format!("RandomBlockProvider[blocks={blocks}]")
        }
        Some("minecraft:rotated") => {
            let direction = match v.get("direction").and_then(Value::as_str) {
                Some(direction) => format!("Optional[{direction}]"),
                None => "Optional.empty".to_owned(),
            };
            format!(
                "RotatedBlockProvider[state={}, direction={direction}]",
                holder(v.get("state"))
            )
        }
        Some("minecraft:copy_properties") => {
            format!("CopyPropertiesProvider[source={}]", holder(v.get("source")))
        }
        Some("minecraft:rule_based") => {
            let rules: Vec<String> = v
                .get("rules")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|rule| {
                    format!(
                        "Rule[ifTrue={}, then={}]",
                        rule.get("if_true")
                            .map(|p| predicate_string(p, ctx))
                            .unwrap_or_default(),
                        holder(rule.get("then"))
                    )
                })
                .collect();
            format!(
                "RuleBasedStateProvider[fallback={}, rules=[{}]]",
                holder(v.get("fallback")),
                rules.join(", ")
            )
        }
        Some("minecraft:weighted") => {
            "WeightedStateProvider[weightedList=net.minecraft.util.random.WeightedList@0]"
                .to_owned()
        }
        Some("minecraft:dual_noise") => format!("{PACKAGE}.DualNoiseProvider@0"),
        Some("minecraft:noise") => format!("{PACKAGE}.NoiseProvider@0"),
        Some("minecraft:noise_threshold") => format!("{PACKAGE}.NoiseThresholdProvider@0"),
        Some("minecraft:randomized_int") => format!("{PACKAGE}.RandomizedIntStateProvider@0"),
        _ => String::new(),
    }
}

/// Java's `toString` of the block predicate an (already validated) predicate JSON decodes to: the records print their
/// fields, the other classes their identity hash (`@0`, see [`provider_string`]).
fn predicate_string(v: &Value, ctx: &RegistryDecodeContext) -> String {
    const PACKAGE: &str = "net.minecraft.world.level.levelgen.blockpredicates";
    let offset = |v: Option<&Value>| {
        let ints: Vec<i32> = v
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_number)
            .map(int_value)
            .collect();
        let at = |i: usize| ints.get(i).copied().unwrap_or(0);
        format!("Vec3i{{x={}, y={}, z={}}}", at(0), at(1), at(2))
    };
    let anchor = |v: Option<&Value>| v.map(|a| vertical_anchor_text(a, ctx)).unwrap_or_default();
    let ty = v.get("type").and_then(Value::as_str).unwrap_or_default();
    match ty.strip_prefix("minecraft:").unwrap_or(ty) {
        "unobstructed" => format!("UnobstructedPredicate[offset={}]", offset(v.get("offset"))),
        "height_range" => format!(
            "HeightRangePredicate[minInclusive={}, maxInclusive={}]",
            anchor(v.get("min_inclusive")),
            anchor(v.get("max_inclusive"))
        ),
        "volume_match" => format!(
            "VolumeMatchPredicate[min={}, max={}, match={}]",
            offset(v.get("min")),
            offset(v.get("max")),
            v.get("match")
                .map(|p| predicate_string(p, ctx))
                .unwrap_or_default()
        ),
        class => {
            let class = match class {
                "all_of" => "AllOfPredicate",
                "any_of" => "AnyOfPredicate",
                "not" => "NotPredicate",
                "true" => "TrueBlockPredicate",
                "matching_blocks" => "MatchingBlocksPredicate",
                "matching_fluids" => "MatchingFluidsPredicate",
                "matching_block_tag" => "MatchingBlockTagPredicate",
                "matching_biomes" => "MatchingBiomesPredicate",
                "has_sturdy_face" => "HasSturdyFacePredicate",
                "solid" => "SolidPredicate",
                "replaceable" => "ReplaceablePredicate",
                "inside_world_bounds" => "InsideWorldBoundsPredicate",
                "would_survive" => "WouldSurvivePredicate",
                other => other,
            };
            format!("{PACKAGE}.{class}@0")
        }
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
pub(crate) fn non_negative_int() -> Schema {
    Schema::custom(|v, ctx| match Schema::Int.convert(v, ctx)? {
        Tag::Int(n) if n < 0 => Err(format!("Value must be non-negative: {n}")),
        other => Ok(other),
    })
}

/// `WeightedList.nonEmptyCodec(BlockState.CODEC)`.
fn weighted_states() -> Schema {
    non_empty_weighted_list(block_state_schema())
}

/// `WeightedList.nonEmptyCodec(data)`: `{data, weight}` entries, at least one with weight. `WeightedList.of` throws when
/// the weights add up to more than `Integer.MAX_VALUE`, before the check for an empty list runs.
pub(super) fn non_empty_weighted_list(data: Schema) -> Schema {
    let entries = Schema::list_of(Schema::Record(vec![
        req("data", data),
        req("weight", non_negative_int()),
    ]));
    Schema::custom_partial(move |v, ctx| {
        validate(entries.decode(v, ctx), |entries| {
            let total: i64 = match entries {
                Tag::List(list) => list
                    .iter()
                    .filter_map(|e| e.as_compound()?.get_int("weight"))
                    .map(i64::from)
                    .sum(),
                _ => 0,
            };
            if total > i64::from(i32::MAX) {
                return Err(illegal_argument("Sum of weights must be <= 2147483647"));
            }
            if total == 0 {
                return Err(
                    "Weighted list must contain at least one entry with non-zero weight".to_owned(),
                );
            }
            Ok(())
        })
    })
}

/// `InclusiveRange.codec(Codec.INT, 1, 64)`, made by `ExtraCodecs.intervalCodec`: a number, a `[min, max]` list or a
/// `{min_inclusive, max_inclusive}` object; equal bounds encode as the single number, else as a list.
fn variety() -> Schema {
    fn bounds(tag: &Tag) -> Vec<i32> {
        match tag {
            Tag::Int(n) => vec![*n],
            Tag::List(list) => list
                .iter()
                .filter_map(|t| match t {
                    Tag::Int(n) => Some(*n),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    let range = interval_codec(Schema::Int, "min_inclusive", "max_inclusive", |min, max| {
        let (Tag::Int(min), Tag::Int(max)) = (min, max) else {
            return Err("Not a number".to_owned());
        };
        // `InclusiveRange.create`
        if min > max {
            return Err("min_inclusive must be less than or equal to max_inclusive".to_owned());
        }
        Ok(if min == max {
            Tag::Int(*min)
        } else {
            Tag::List(
                [Tag::Int(*min), Tag::Int(*max)]
                    .into_iter()
                    .collect::<mc_nbt::ListTag>(),
            )
        })
    });
    Schema::custom_partial(move |v, ctx| {
        validate(range.decode(v, ctx), |tag| {
            let bounds = bounds(tag);
            let (Some(min), Some(max)) = (bounds.first(), bounds.last()) else {
                return Ok(());
            };
            if *min < 1 {
                Err(format!(
                    "Range limit too low, expected at least 1 [{min}-{max}]"
                ))
            } else if *max > 64 {
                Err(format!(
                    "Range limit too high, expected at most 64 [{min}-{max}]"
                ))
            } else {
                Ok(())
            }
        })
    })
}

/// `NormalNoise.Normalization.CODEC`: a boolean or the string `legacy`.
fn normalization() -> Schema {
    Schema::either(
        Schema::Bool,
        Schema::custom(|v, _| match v {
            Value::String(s) if s == "legacy" => Ok(Tag::String(s.clone().into())),
            Value::String(s) => Err(format!("Invalid normalization type: {s}")),
            _ => Err(format!("Not a string: {v}")),
        }),
    )
}

/// `NormalNoise.Parameters.CODEC` (with its `validate`).
pub(super) fn noise_parameters() -> Schema {
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
            Schema::sized_list(Schema::DoubleRange(0.0, 1_000_000.0), 0, 32),
            Value::Array(Vec::new()),
        ),
    ]);
    Schema::custom_partial(move |v, ctx| {
        validate(record.decode(v, ctx), |tag| {
            if let Tag::Compound(c) = tag {
                let modifiers = c.get_list("amplitude_modifiers").map_or(0, |l| l.len());
                let octaves = c.get_int("octave_count").unwrap_or(1);
                if modifiers != 0 && modifiers as i32 != octaves {
                    return Err(format!(
                        "amplitude_modifiers had size {modifiers}, but octave_count was {octaves}"
                    ));
                }
            }
            Ok(())
        })
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
    Schema::sized_list(transform, 1, 200)
}
