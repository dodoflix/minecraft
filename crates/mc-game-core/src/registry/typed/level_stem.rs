//! `world.level.dimension.LevelStem`: the `dimension` registry (dimension type + chunk generator).
//!
//! This is the registry-stage *codec*: it validates a stem exactly like `LevelStem.CODEC` against the registries
//! (`RegistryOps` lookups, inline dimension types, inline biomes, flat settings) and produces the canonical NBT the codec
//! encodes (what `world_gen_settings.dat` stores). The typed stem with its built chunk generator is
//! `mc_worldgen::world_preset::LevelStem`, created by [`crate::server::world_dimensions`] when the dimensions are baked.

use mc_nbt::{CompoundTag, Tag};
use serde_json::Value;

use super::biome;
use super::dimension_type::{DimensionType, Y_SIZE};
use crate::registry::codec::{RegistryDecodeContext, Schema, default, req};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// `Climate.ParameterPoint.CODEC` `parse` + `encodeStart`: values are quantized (`(long)(v * 10000)`) and encode back as
/// floats; an interval with equal bounds encodes as its single number.
fn parameter_point(v: &Value, _ctx: &RegistryDecodeContext) -> Result<Tag, String> {
    use mc_worldgen::biome::climate::{ParameterPoint, unquantize_coord};
    let point = ParameterPoint::from_json(v).map_err(|e| e.to_string())?;
    let mut out = CompoundTag::new();
    let names = [
        "temperature",
        "humidity",
        "continentalness",
        "erosion",
        "depth",
        "weirdness",
    ];
    let params = [
        point.temperature,
        point.humidity,
        point.continentalness,
        point.erosion,
        point.depth,
        point.weirdness,
    ];
    for (name, p) in names.iter().zip(params) {
        let (min, max) = (unquantize_coord(p.min), unquantize_coord(p.max));
        let tag = if p.min == p.max {
            Tag::Float(min)
        } else {
            Tag::List(vec![Tag::Float(min), Tag::Float(max)].into())
        };
        out.put(*name, tag);
    }
    out.put("offset", Tag::Float(unquantize_coord(point.offset)));
    Ok(Tag::Compound(out))
}

fn biome_source_types(ty: &str) -> Option<Schema> {
    use Schema as S;
    Some(match ty {
        "minecraft:fixed" => S::Record(vec![req("biome", biome::holder_schema())]),
        "minecraft:checkerboard" => S::Record(vec![
            req("biomes", biome::holder_set_schema()),
            default("scale", S::IntRange(0, 62), Value::from(2)),
        ]),
        "minecraft:the_end" => S::Record(vec![]),
        "minecraft:multi_noise" => S::custom(|v, ctx| {
            // either a preset reference or an explicit biome list
            if v.get("preset").is_some() {
                S::Record(vec![req(
                    "preset",
                    S::Holder(keys::MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST),
                )])
                .convert(v, ctx)
            } else {
                S::Record(vec![req(
                    "biomes",
                    S::list_of(S::Record(vec![
                        req("biome", biome::holder_schema()),
                        req("parameters", S::custom(parameter_point)),
                    ])),
                )])
                .convert(v, ctx)
            }
        }),
        _ => return None,
    })
}

/// `FlatLevelGeneratorSettings.CODEC` (`parse` + `encodeStart`): `lakes`, `features` and `biome` are always written (a
/// missing or undecodable biome encodes as the default `minecraft:plains`), `structure_overrides` and the biome are lenient.
fn flat_settings(
    v: &Value,
    ctx: &RegistryDecodeContext,
    validate_height: bool,
) -> Result<Tag, String> {
    let o = v
        .as_object()
        .ok_or_else(|| format!("Not a JSON object: {v}"))?;
    let mut out = CompoundTag::new();
    if let Some(tag) = o.get("structure_overrides").and_then(|s| {
        Schema::HolderSet {
            registry: keys::STRUCTURE_SET,
            always_list: false,
        }
        .convert(s, ctx)
        .ok()
    }) {
        out.put("structure_overrides", tag);
    }
    let layer = Schema::Record(vec![
        req("height", Schema::IntRange(0, Y_SIZE)),
        req("block", Schema::StaticRegistry(keys::BLOCK)),
    ]);
    let layers = o
        .get("layers")
        .ok_or_else(|| format!("No key layers in MapLike[{v}]"))?;
    let layers = Schema::list_of(layer).convert(layers, ctx)?;
    // `validateHeight`: `IntStream.sum` wraps
    let total = match &layers {
        Tag::List(list) => list.iter().fold(0i32, |sum, l| {
            sum.wrapping_add(if let Tag::Compound(c) = l {
                c.get_int("height").unwrap_or(0)
            } else {
                0
            })
        }),
        _ => 0,
    };
    if validate_height && total > Y_SIZE {
        return Err(format!("Sum of layer heights is > {Y_SIZE}"));
    }
    out.put("layers", layers);
    for flag in ["lakes", "features"] {
        out.put(
            flag,
            Schema::Bool.convert(o.get(flag).unwrap_or(&Value::Bool(false)), ctx)?,
        );
    }
    let biome = o
        .get("biome")
        .and_then(|b| biome::holder_schema().convert(b, ctx).ok());
    out.put(
        "biome",
        biome.unwrap_or_else(|| Tag::String("minecraft:plains".to_owned())),
    );
    Ok(Tag::Compound(out))
}

fn generator_types(ty: &str) -> Option<Schema> {
    use Schema as S;
    Some(match ty {
        "minecraft:noise" => S::Record(vec![
            req(
                "settings",
                S::custom(|v, ctx| match v {
                    Value::String(_) => S::Holder(keys::NOISE_SETTINGS).convert(v, ctx),
                    _ => Ok(super::super::codec::json_to_nbt(v)),
                }),
            ),
            req(
                "biome_source",
                S::Dispatch {
                    key: "type",
                    resolve: biome_source_types,
                },
            ),
        ]),
        "minecraft:flat" => S::Record(vec![req(
            "settings",
            S::custom(|v, ctx| flat_settings(v, ctx, true)),
        )]),
        "minecraft:debug" => S::Record(vec![]),
        _ => return None,
    })
}

fn schema() -> Schema {
    Schema::Record(vec![
        req(
            "type",
            Schema::custom(|v, ctx| match v {
                Value::String(_) => Schema::Holder(keys::DIMENSION_TYPE).convert(v, ctx),
                _ => DimensionType::from_json_with_nbt(v, ctx).map(|(_, tag)| tag),
            }),
        ),
        req(
            "generator",
            Schema::Dispatch {
                key: "type",
                resolve: generator_types,
            },
        ),
    ])
}

/// The `minecraft:flat` generator compound of `settings` without the final `validateHeight` check: what
/// `resultOrPartial` keeps of settings whose layers are too tall (`DataResult.error(msg, settings)`).
pub fn flat_generator_tag(
    settings: &Value,
    ctx: &RegistryDecodeContext,
    validate_height: bool,
) -> Result<Tag, String> {
    let mut generator = CompoundTag::new();
    generator.put("settings", flat_settings(settings, ctx, validate_height)?);
    generator.put("type", Tag::String("minecraft:flat".to_owned()));
    Ok(Tag::Compound(generator))
}

fn unknown_generator_type(e: String) -> String {
    match e.strip_prefix("Unknown type ") {
        Some(ty) => format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:worldgen/chunk_generator]: {ty}"
        ),
        None => e,
    }
}

/// `LevelStem.CODEC` `parse` + `encodeStart`: validates `json` and returns the canonical stem compound.
pub fn encode(json: &Value, ctx: &RegistryDecodeContext) -> Result<Tag, String> {
    schema().convert(json, ctx).map_err(unknown_generator_type)
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    // validation only: the typed stems (with their chunk generators) are built when the dimensions are baked
    encode(json, ctx).map(|_| DecodedEntry::new(()))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder {
    registry: keys::LEVEL_STEM,
    decode,
};
