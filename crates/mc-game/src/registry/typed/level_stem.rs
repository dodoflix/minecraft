//! `world.level.dimension.LevelStem`: the `dimension` registry (dimension type + chunk generator).
//!
//! The generator is kept as validated JSON with typed accessors; the chunk generator itself is built by the worldgen
//! slice from [`LevelStem::generator`].

use std::sync::Arc;

use mc_core::ident::Identifier;
use serde_json::Value;

use super::dimension_type::DimensionType;
use super::get_str;
use crate::registry::codec::{RegistryDecodeContext, Schema, default, req};
use crate::registry::decoders::{DYNAMIC_REGISTRY_DECODERS, DecodedEntry, DynamicRegistryDecoder};
use crate::registry::keys;

/// `LevelStem.type`: a reference to a `dimension_type` or an inline definition.
#[derive(Clone, Debug)]
pub enum DimensionTypeSource {
    Reference(Identifier),
    Inline(Arc<DimensionType>),
}

/// `LevelStem`.
#[derive(Clone, Debug)]
pub struct LevelStem {
    pub dimension_type: DimensionTypeSource,
    /// The `generator` object (`type` = `minecraft:noise` / `minecraft:flat` / `minecraft:debug`).
    pub generator: Arc<Value>,
}

/// Reference to the noise settings of a `minecraft:noise` generator.
#[derive(Clone, Debug)]
pub enum NoiseSettingsSource<'a> {
    Reference(Identifier),
    Inline(&'a Value),
}

impl LevelStem {
    /// `ChunkGenerator` type id, e.g. `minecraft:noise`.
    pub fn generator_type(&self) -> &str {
        get_str(&self.generator, "type").unwrap_or_default()
    }

    /// `settings` of a noise generator (`NoiseGeneratorSettings` reference or inline).
    pub fn noise_settings(&self) -> Option<NoiseSettingsSource<'_>> {
        let settings = self.generator.get("settings")?;
        match settings {
            Value::String(s) => Identifier::try_parse(s).map(NoiseSettingsSource::Reference),
            other => Some(NoiseSettingsSource::Inline(other)),
        }
    }

    /// `biome_source` object of a noise generator.
    pub fn biome_source(&self) -> Option<&Value> {
        self.generator.get("biome_source")
    }
}

fn biome_source_types(ty: &str) -> Option<Schema> {
    use Schema as S;
    Some(match ty {
        "minecraft:fixed" => S::Record(vec![req("biome", S::Holder(keys::BIOME))]),
        "minecraft:checkerboard" => S::Record(vec![req("biomes", S::HolderSet { registry: keys::BIOME, always_list: false }), default("scale", S::IntRange(0, 62), Value::from(2))]),
        "minecraft:the_end" => S::Record(vec![]),
        "minecraft:multi_noise" => S::custom(|v, ctx| {
            // either a preset reference or an explicit biome list
            if v.get("preset").is_some() {
                S::Record(vec![req("preset", S::Holder(keys::MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST))]).convert(v, ctx)
            } else {
                S::Record(vec![req("biomes", S::list_of(S::Record(vec![req("biome", S::Holder(keys::BIOME)), req("parameters", S::Raw)])))]).convert(v, ctx)
            }
        }),
        _ => return None,
    })
}

fn generator_types(ty: &str) -> Option<Schema> {
    use Schema as S;
    Some(match ty {
        "minecraft:noise" => S::Record(vec![
            req("settings", S::custom(|v, ctx| match v {
                Value::String(_) => S::Holder(keys::NOISE_SETTINGS).convert(v, ctx),
                _ => Ok(super::super::codec::json_to_nbt(v)),
            })),
            req("biome_source", S::Dispatch { key: "type", resolve: biome_source_types }),
        ]),
        "minecraft:flat" => S::Record(vec![req("settings", S::Raw)]),
        "minecraft:debug" => S::Record(vec![]),
        _ => return None,
    })
}

fn schema() -> Schema {
    Schema::Record(vec![
        req("type", Schema::custom(|v, ctx| match v {
            Value::String(_) => Schema::Holder(keys::DIMENSION_TYPE).convert(v, ctx),
            Value::Object(_) => {
                DimensionType::from_json(v, ctx)?;
                Ok(super::super::codec::json_to_nbt(v))
            }
            other => Err(format!("Not a string: {other}")),
        })),
        req("generator", Schema::Dispatch { key: "type", resolve: generator_types }),
    ])
}

impl LevelStem {
    pub fn from_json(json: &Value, ctx: &RegistryDecodeContext) -> Result<LevelStem, String> {
        schema().convert(json, ctx).map_err(|e| {
            if e.starts_with("Unknown type ") {
                format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:worldgen/chunk_generator]: {}", e.trim_start_matches("Unknown type "))
            } else {
                e
            }
        })?;
        let dimension_type = match json.get("type") {
            Some(Value::String(s)) => DimensionTypeSource::Reference(Identifier::read(s)?),
            Some(inline) => DimensionTypeSource::Inline(Arc::new(DimensionType::from_json(inline, ctx)?)),
            None => return Err("No type".to_owned()),
        };
        Ok(LevelStem { dimension_type, generator: Arc::new(json.get("generator").cloned().unwrap_or(Value::Null)) })
    }
}

fn decode(json: &Value, ctx: &RegistryDecodeContext) -> Result<DecodedEntry, String> {
    Ok(DecodedEntry::new(LevelStem::from_json(json, ctx)?))
}

#[linkme::distributed_slice(DYNAMIC_REGISTRY_DECODERS)]
static DECODER: DynamicRegistryDecoder = DynamicRegistryDecoder { registry: keys::LEVEL_STEM, decode };

