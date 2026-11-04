//! The biome registry: `Biome` (climate settings + generation settings), `BiomeGenerationSettings` and the
//! registry holding every `worldgen/biome` entry in vanilla's (dedicated server) id order.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use serde_json::Value;

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::random::{LegacyRandomSource, WorldgenRandom};
use mc_data::block::BlockState;
use mc_world::palette::BiomeId;

use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::codec::{Fields, LoadResult, parse_bool, parse_f32, parse_identifier, parse_list, parse_str};
use crate::synth::{Noise, NoiseStack, SimplexNoise};

/// The registry folder of biomes.
pub const BIOME_REGISTRY: &str = "worldgen/biome";
const CARVER_TAG_DIR: &str = "worldgen/carver";
const PLACED_FEATURE_TAG_DIR: &str = "worldgen/placed_feature";
const BIOME_TAG_DIR: &str = "worldgen/biome";

static TEMPERATURE_NOISE: LazyLock<SimplexNoise> = LazyLock::new(|| {
    SimplexNoise::new(&mut WorldgenRandom::new(Box::new(LegacyRandomSource::new(1234))), true)
});

/// `Biome.FROZEN_TEMPERATURE_NOISE`.
pub static FROZEN_TEMPERATURE_NOISE: LazyLock<NoiseStack> = LazyLock::new(|| {
    let mut random = WorldgenRandom::new(Box::new(LegacyRandomSource::new(3456)));
    let first = Arc::new(SimplexNoise::new(&mut random, true));
    let second = Arc::new(SimplexNoise::new(&mut random, true));
    let third = Arc::new(SimplexNoise::new(&mut random, true));
    NoiseStack::builder()
        .add(first, 1.0, 0.14285715)
        .add(second, 0.5, 0.2857143)
        .add(third, 0.25, 0.5714286)
        .build()
});

/// `Biome.BIOME_INFO_NOISE`.
pub static BIOME_INFO_NOISE: LazyLock<SimplexNoise> = LazyLock::new(|| {
    SimplexNoise::new(&mut WorldgenRandom::new(Box::new(LegacyRandomSource::new(2345))), true)
});

/// `Biome.TemperatureModifier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TemperatureModifier {
    None,
    Frozen,
}

impl TemperatureModifier {
    /// `TemperatureModifier.CODEC` (`none` / `frozen`).
    pub fn from_name(name: &str) -> Option<TemperatureModifier> {
        match name {
            "none" => Some(TemperatureModifier::None),
            "frozen" => Some(TemperatureModifier::Frozen),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TemperatureModifier::None => "none",
            TemperatureModifier::Frozen => "frozen",
        }
    }

    /// `modifyTemperature(pos, baseTemperature)`.
    pub fn modify_temperature(self, pos: BlockPos, base_temperature: f32) -> f32 {
        match self {
            TemperatureModifier::None => base_temperature,
            TemperatureModifier::Frozen => {
                let x = f64::from(pos.x());
                let z = f64::from(pos.z());
                let ground_value_large_variation =
                    f64::from(FROZEN_TEMPERATURE_NOISE.get2(x * 0.05, z * 0.05) * 7.0f32);
                let ground_value_edge_variation = f64::from(BIOME_INFO_NOISE.get2(x * 0.2, z * 0.2));
                let ice_patches = ground_value_large_variation + ground_value_edge_variation;
                if ice_patches < 0.3 {
                    let ground_value_small_variation = f64::from(BIOME_INFO_NOISE.get2(x * 0.09, z * 0.09));
                    if ground_value_small_variation < 0.8 {
                        return 0.2;
                    }
                }
                base_temperature
            }
        }
    }
}

/// `Biome.Precipitation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

/// `Biome.ClimateSettings`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimateSettings {
    pub has_precipitation: bool,
    pub temperature: f32,
    pub temperature_modifier: TemperatureModifier,
    pub downfall: f32,
}

impl ClimateSettings {
    fn from_json(value: &Value) -> LoadResult<ClimateSettings> {
        let fields = Fields::new(value)?;
        Ok(ClimateSettings {
            has_precipitation: fields.required("has_precipitation", parse_bool)?,
            temperature: fields.required("temperature", parse_f32)?,
            temperature_modifier: fields
                .optional("temperature_modifier", |v| {
                    let name = parse_str(v)?;
                    TemperatureModifier::from_name(name)
                        .ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
                })?
                .unwrap_or(TemperatureModifier::None),
            downfall: fields.required("downfall", parse_f32)?,
        })
    }
}

/// A placed feature of a biome's feature steps: a registry reference, or the inline fill-layer feature flat
/// worlds add (`FlatLevelGeneratorSettings.adjustGenerationSettings`).
#[derive(Clone, Debug, PartialEq)]
pub enum PlacedFeatureRef {
    Named(Identifier),
    InlineFillLayer { height: i32, state: BlockState },
}

/// `BiomeGenerationSettings`: carvers (flattened `HolderSet<WorldCarver>`) and the features of every
/// `GenerationStep.Decoration`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BiomeGenerationSettings {
    pub carvers: Vec<Identifier>,
    pub features: Vec<Vec<PlacedFeatureRef>>,
}

impl BiomeGenerationSettings {
    /// `BiomeGenerationSettings.EMPTY` is `BiomeGenerationSettings::default()`.
    pub fn new(carvers: Vec<Identifier>, features: Vec<Vec<PlacedFeatureRef>>) -> BiomeGenerationSettings {
        BiomeGenerationSettings { carvers, features }
    }

    /// `getCarvers()`.
    pub fn get_carvers(&self) -> &[Identifier] {
        &self.carvers
    }

    /// `features()`.
    pub fn features(&self) -> &[Vec<PlacedFeatureRef>] {
        &self.features
    }

    /// `BiomeGenerationSettings.CODEC` (carvers: id | `#tag` | list; features: list of those).
    fn from_json(biome: &Value) -> LoadResult<BiomeGenerationSettings> {
        let fields = Fields::new(biome)?;
        let carvers = fields.required("carvers", |v| holder_set_names(v, CARVER_TAG_DIR))?;
        let features = fields.required("features", |v| {
            parse_list(v, |step| {
                Ok(holder_set_names(step, PLACED_FEATURE_TAG_DIR)?
                    .into_iter()
                    .map(PlacedFeatureRef::Named)
                    .collect())
            })
        })?;
        Ok(BiomeGenerationSettings { carvers, features })
    }
}

/// A `HolderSet` of registry entries by id: a single id, a `#tag` (resolved against the vanilla tags of
/// `tag_dir`) or a list of ids (`HolderSetCodec`; element holders must be references).
fn holder_set_names(value: &Value, tag_dir: &str) -> LoadResult<Vec<Identifier>> {
    match value {
        Value::String(s) => {
            if let Some(tag) = s.strip_prefix('#') {
                let tag = Identifier::read(tag).map_err(LoadError::new)?;
                let names = mc_data::tags::names(tag_dir, &tag.to_string())
                    .ok_or_else(|| LoadError::new(format!("Missing tag: '{tag}' in 'minecraft:{tag_dir}'")))?;
                names
                    .iter()
                    .map(|n| Identifier::read(n).map_err(LoadError::new))
                    .collect()
            } else {
                Ok(vec![parse_identifier(value)?])
            }
        }
        Value::Array(_) => parse_list(value, |e| match e {
            Value::String(_) => parse_identifier(e),
            _ => Err(LoadError::new("inline registry entries are not supported here")),
        }),
        _ => Err(LoadError::new("expected id, tag or list")),
    }
}

/// `Biome`.
#[derive(Clone, Debug)]
pub struct Biome {
    climate: ClimateSettings,
    generation_settings: BiomeGenerationSettings,
    /// `attributes` (`EnvironmentAttributeMap`, incl. the natural mob spawns) as raw JSON, if present.
    pub attributes: Option<Value>,
    /// `effects` (`BiomeSpecialEffects`) as raw JSON.
    pub effects: Value,
}

impl Biome {
    /// `Biome.DIRECT_CODEC`.
    pub fn from_json(value: &Value) -> LoadResult<Biome> {
        let fields = Fields::new(value)?;
        let climate = ClimateSettings::from_json(value)?;
        let attributes = fields.optional("attributes", |v| Ok(v.clone()))?;
        let effects = fields.required("effects", |v| Ok(v.clone()))?;
        let generation_settings = BiomeGenerationSettings::from_json(value)?;
        Ok(Biome {
            climate,
            generation_settings,
            attributes,
            effects,
        })
    }

    pub fn climate(&self) -> &ClimateSettings {
        &self.climate
    }

    /// `getGenerationSettings()`.
    pub fn generation_settings(&self) -> &BiomeGenerationSettings {
        &self.generation_settings
    }

    /// `hasPrecipitation()`.
    pub fn has_precipitation(&self) -> bool {
        self.climate.has_precipitation
    }

    /// `getBaseTemperature()`.
    pub fn get_base_temperature(&self) -> f32 {
        self.climate.temperature
    }

    /// `getPrecipitationAt(pos, seaLevel)`.
    pub fn get_precipitation_at(&self, pos: BlockPos, sea_level: i32) -> Precipitation {
        if !self.has_precipitation() {
            Precipitation::None
        } else if self.cold_enough_to_snow(pos, sea_level) {
            Precipitation::Snow
        } else {
            Precipitation::Rain
        }
    }

    /// `getHeightAdjustedTemperature(pos, seaLevel)`.
    fn get_height_adjusted_temperature(&self, pos: BlockPos, sea_level: i32) -> f32 {
        let adjusted_temperature = self
            .climate
            .temperature_modifier
            .modify_temperature(pos, self.get_base_temperature());
        let snow_level = sea_level.wrapping_add(17);
        if pos.y() > snow_level {
            let v = TEMPERATURE_NOISE.get2(
                f64::from(pos.x() as f32 / 8.0f32),
                f64::from(pos.z() as f32 / 8.0f32),
            ) * 8.0f32;
            adjusted_temperature
                - (v + pos.y() as f32 - snow_level as f32) * 0.05f32 / 40.0f32
        } else {
            adjusted_temperature
        }
    }

    /// `getTemperature(pos, seaLevel)` (vanilla's per-thread cache of this value is not observable).
    fn get_temperature(&self, pos: BlockPos, sea_level: i32) -> f32 {
        self.get_height_adjusted_temperature(pos, sea_level)
    }

    /// `coldEnoughToSnow(pos, seaLevel)`.
    pub fn cold_enough_to_snow(&self, pos: BlockPos, sea_level: i32) -> bool {
        !self.warm_enough_to_rain(pos, sea_level)
    }

    /// `warmEnoughToRain(pos, seaLevel)`.
    pub fn warm_enough_to_rain(&self, pos: BlockPos, sea_level: i32) -> bool {
        self.get_temperature(pos, sea_level) >= 0.15f32
    }

    /// `shouldMeltFrozenOceanIcebergSlightly(pos, seaLevel)`.
    pub fn should_melt_frozen_ocean_iceberg_slightly(&self, pos: BlockPos, sea_level: i32) -> bool {
        self.get_temperature(pos, sea_level) > 0.1f32
    }
}

/// The biome registry as the dedicated server builds it from the vanilla data pack: entries are
/// registered in `worldgen/biome` resource-listing order, which is the byte order of the identifiers, so
/// `BiomeId(i)` is the i-th identifier in sorted order (67 entries in 26.3, `minecraft:badlands` = 0 ...
/// `minecraft:wooded_badlands` = 66).
#[derive(Debug)]
pub struct BiomeRegistry {
    biomes: Vec<Biome>,
    keys: Vec<Identifier>,
    ids: HashMap<Identifier, BiomeId>,
}

impl BiomeRegistry {
    /// Loads every `worldgen/biome` entry of the data source (in sorted identifier order).
    pub fn load(registries: &WorldgenRegistries) -> LoadResult<BiomeRegistry> {
        let mut keys = registries.list(BIOME_REGISTRY);
        keys.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        let mut biomes = Vec::with_capacity(keys.len());
        for key in &keys {
            let json = registries
                .raw(BIOME_REGISTRY, key)
                .ok_or_else(|| LoadError::new("no such registry entry").in_entry(BIOME_REGISTRY, key))?;
            biomes.push(Biome::from_json(&json).map_err(|e| e.in_entry(BIOME_REGISTRY, key))?);
        }
        let ids = keys
            .iter()
            .enumerate()
            .map(|(i, key)| (key.clone(), BiomeId(i as u32)))
            .collect();
        Ok(BiomeRegistry { biomes, keys, ids })
    }

    pub fn len(&self) -> usize {
        self.biomes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.biomes.is_empty()
    }

    /// `getId(key)` as an id.
    pub fn id_of(&self, key: &Identifier) -> Option<BiomeId> {
        self.ids.get(key).copied()
    }

    /// `getId` by a path in the `minecraft` namespace (`"plains"`).
    pub fn id_of_name(&self, name: &str) -> Option<BiomeId> {
        self.id_of(&Identifier::try_parse(name)?)
    }

    /// `getKey(biome)`. Panics on an id outside the registry.
    pub fn key(&self, id: BiomeId) -> &Identifier {
        &self.keys[id.0 as usize]
    }

    /// `byId(id)`. Panics on an id outside the registry.
    pub fn get(&self, id: BiomeId) -> &Biome {
        &self.biomes[id.0 as usize]
    }

    /// All ids in registry order.
    pub fn ids(&self) -> impl Iterator<Item = BiomeId> + use<> {
        (0..self.biomes.len() as u32).map(BiomeId)
    }

    /// The registry names in id order (for `mc_world::palette::BiomeRegistry::new`).
    pub fn names(&self) -> Vec<String> {
        self.keys.iter().map(|k| k.to_string()).collect()
    }

    /// `Biome.CODEC` as a reference: a registered biome id (inline biome definitions are not registry ids).
    pub fn holder_from_json(&self, value: &Value) -> LoadResult<BiomeId> {
        let id = parse_identifier(value)?;
        self.id_of(&id)
            .ok_or_else(|| LoadError::new(format!("Failed to get element {id}")))
    }

    /// `Biome.LIST_CODEC` (`HolderSetCodec`): a single biome id, a `#tag` or a list of biome ids, flattened in
    /// order.
    pub fn holder_set_from_json(&self, value: &Value) -> LoadResult<Vec<BiomeId>> {
        match value {
            Value::String(s) if s.starts_with('#') => {
                let tag = Identifier::read(&s[1..]).map_err(LoadError::new)?;
                let names = mc_data::tags::names(BIOME_TAG_DIR, &tag.to_string())
                    .ok_or_else(|| LoadError::new(format!("Missing tag: '{tag}' in 'minecraft:worldgen/biome'")))?;
                names
                    .iter()
                    .map(|n| {
                        let id = Identifier::read(n).map_err(LoadError::new)?;
                        self.id_of(&id)
                            .ok_or_else(|| LoadError::new(format!("Failed to get element {id}")))
                    })
                    .collect()
            }
            Value::String(_) => Ok(vec![self.holder_from_json(value)?]),
            Value::Array(_) => parse_list(value, |e| self.holder_from_json(e)),
            _ => Err(LoadError::new("expected biome id, tag or list")),
        }
    }
}
