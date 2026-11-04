//! `LevelStem`, `WorldPreset`/`WorldPresets`, `WorldDimensions` and the dimension `generator` factory
//! (`ChunkGenerator.CODEC`: `minecraft:noise`, `minecraft:flat`, `minecraft:debug`).

use std::sync::Arc;

use mc_core::ident::Identifier;
use serde_json::{Map, Value};

use crate::biome::{BiomeRegistry, BiomeSource};
use crate::data::{LoadError, WorldgenRegistries};
use crate::debug_level_source::DebugLevelSource;
use crate::densityfunction::codec::{Fields, LoadResult, parse_identifier};
use crate::flat::settings::{FlatLevelGeneratorSettings, get_default_biome};
use crate::flat::{FlatLayerInfo, FlatLevelSource};
use crate::generator::{ChunkGenerator, NoiseBasedChunkGenerator};
use crate::noise_settings::NoiseGeneratorSettings;

pub const DIMENSION_TYPE_REGISTRY: &str = "dimension_type";
pub const WORLD_PRESET_REGISTRY: &str = "worldgen/world_preset";

fn minecraft(path: &str) -> Identifier {
    Identifier::with_default_namespace(path).expect("valid identifier")
}

// ---- generators -------------------------------------------------------------------------------------

/// `NoiseBasedChunkGenerator` plus what its codec was given (the generator itself keeps no ids).
pub struct NoiseGenerator {
    pub generator: NoiseBasedChunkGenerator,
    /// `type` of the biome source codec, e.g. `minecraft:multi_noise`.
    pub biome_source_type: Identifier,
    /// The `settings` reference (`None` for inline settings).
    pub settings: Option<Identifier>,
}

/// The three vanilla `ChunkGenerator` implementations a dimension can use.
pub enum LevelGenerator {
    Noise(NoiseGenerator),
    Flat(FlatLevelSource),
    Debug(DebugLevelSource),
}

impl LevelGenerator {
    /// `ChunkGenerator.getTypeNameForDataFixer`: the id in the chunk-generator registry.
    pub fn type_name(&self) -> Identifier {
        minecraft(match self {
            LevelGenerator::Noise(_) => "noise",
            LevelGenerator::Flat(_) => "flat",
            LevelGenerator::Debug(_) => "debug",
        })
    }

    pub fn as_chunk_generator(&self) -> &dyn ChunkGenerator {
        match self {
            LevelGenerator::Noise(n) => &n.generator,
            LevelGenerator::Flat(f) => f,
            LevelGenerator::Debug(d) => d,
        }
    }

    pub fn into_boxed(self) -> Box<dyn ChunkGenerator> {
        match self {
            LevelGenerator::Noise(n) => Box::new(n.generator),
            LevelGenerator::Flat(f) => Box::new(f),
            LevelGenerator::Debug(d) => Box::new(d),
        }
    }
}

/// `ChunkGenerator.CODEC`: `{"type": "minecraft:noise" | "minecraft:flat" | "minecraft:debug", ...}`.
pub fn level_generator_from_json(
    value: &Value,
    registries: &Arc<WorldgenRegistries>,
    biomes: &Arc<BiomeRegistry>,
) -> LoadResult<LevelGenerator> {
    let fields = Fields::new(value)?;
    let ty = fields.required("type", parse_identifier)?;
    match ty.to_string().as_str() {
        "minecraft:noise" => {
            let biome_source_json = fields.required("biome_source", Ok)?;
            let source_fields = Fields::new(biome_source_json).map_err(|e| e.at("biome_source"))?;
            let biome_source_type = source_fields.required("type", parse_identifier).map_err(|e| e.at("biome_source"))?;
            let biome_source: Arc<dyn BiomeSource> =
                crate::biome::biome_source_from_json(biome_source_json, registries, biomes).map_err(|e| e.at("biome_source"))?;
            let (settings, settings_id) = fields.required("settings", |v| match v {
                Value::String(_) => {
                    let id = parse_identifier(v)?;
                    Ok((registries.noise_settings(&id)?, Some(id)))
                }
                _ => Ok((NoiseGeneratorSettings::from_json(v, registries)?, None)),
            })?;
            let generator =
                NoiseBasedChunkGenerator::new(biome_source, Arc::new(settings), registries.clone(), biomes.clone())?;
            Ok(LevelGenerator::Noise(NoiseGenerator { generator, biome_source_type, settings: settings_id }))
        }
        "minecraft:flat" => {
            let settings = fields.required("settings", |v| FlatLevelGeneratorSettings::from_json(v, registries, biomes))?;
            Ok(LevelGenerator::Flat(FlatLevelSource::new(settings, biomes.clone())))
        }
        "minecraft:debug" => Ok(LevelGenerator::Debug(DebugLevelSource::new(get_default_biome(biomes)?, biomes.clone()))),
        other => Err(LoadError::new(format!("Unknown registry key in minecraft:chunk_generator: {other}")).at("type")),
    }
}

/// The dimension `generator` factory returning the generator as a trait object.
pub fn generator_from_json(
    value: &Value,
    registries: &Arc<WorldgenRegistries>,
    biomes: &Arc<BiomeRegistry>,
) -> LoadResult<Box<dyn ChunkGenerator>> {
    level_generator_from_json(value, registries, biomes).map(LevelGenerator::into_boxed)
}

// ---- level stems ------------------------------------------------------------------------------------

/// `Holder<DimensionType>`: a registered id or an inline definition (kept as raw JSON; dimension types are decoded by the game).
#[derive(Clone, Debug, PartialEq)]
pub enum DimensionTypeRef {
    Named(Identifier),
    Inline(Value),
}

/// `LevelStem`.
pub struct LevelStem {
    pub dimension_type: DimensionTypeRef,
    pub generator: LevelGenerator,
}

impl LevelStem {
    pub fn from_json(
        value: &Value,
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
    ) -> LoadResult<LevelStem> {
        let fields = Fields::new(value)?;
        let dimension_type = fields.required("type", |v| match v {
            Value::String(_) => {
                let id = parse_identifier(v)?;
                if registries.raw(DIMENSION_TYPE_REGISTRY, &id).is_none() {
                    return Err(LoadError::new(format!("Unknown registry key in minecraft:dimension_type: {id}")));
                }
                Ok(DimensionTypeRef::Named(id))
            }
            _ => Ok(DimensionTypeRef::Inline(v.clone())),
        })?;
        let generator = fields.required("generator", |v| level_generator_from_json(v, registries, biomes))?;
        Ok(LevelStem { dimension_type, generator })
    }
}

pub fn overworld_key() -> Identifier {
    minecraft("overworld")
}

pub fn nether_key() -> Identifier {
    minecraft("the_nether")
}

pub fn end_key() -> Identifier {
    minecraft("the_end")
}

/// `WorldDimensions.keysInOrder`: overworld, nether, end (when known), then the rest in the given order.
pub fn keys_in_order(known: &[Identifier]) -> Vec<Identifier> {
    let builtin = [overworld_key(), nether_key(), end_key()];
    let mut out: Vec<Identifier> = builtin.iter().filter(|k| known.contains(k)).cloned().collect();
    out.extend(known.iter().filter(|k| !builtin.contains(k)).cloned());
    out
}

// ---- WorldDimensions --------------------------------------------------------------------------------

/// `PrimaryLevelData.SpecialWorldProperty`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialWorldProperty {
    None,
    Flat,
    Debug,
}

/// `WorldDimensions`: dimension key -> stem, with a mandatory overworld.
#[derive(Clone)]
pub struct WorldDimensions {
    dimensions: Vec<(Identifier, Arc<LevelStem>)>,
}

impl WorldDimensions {
    /// `new WorldDimensions(map)`: "Overworld settings missing" without an overworld.
    pub fn new(dimensions: Vec<(Identifier, Arc<LevelStem>)>) -> Result<WorldDimensions, LoadError> {
        if !dimensions.iter().any(|(k, _)| *k == overworld_key()) {
            return Err(LoadError::new("Overworld settings missing"));
        }
        Ok(WorldDimensions { dimensions })
    }

    pub fn dimensions(&self) -> &[(Identifier, Arc<LevelStem>)] {
        &self.dimensions
    }

    pub fn get(&self, key: &Identifier) -> Option<&Arc<LevelStem>> {
        self.dimensions.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn overworld(&self) -> &Arc<LevelStem> {
        self.get(&overworld_key()).expect("checked at construction")
    }

    /// `isDebug`.
    pub fn is_debug(&self) -> bool {
        matches!(self.overworld().generator, LevelGenerator::Debug(_))
    }

    /// `specialWorldProperty`.
    pub fn special_world_property(&self) -> SpecialWorldProperty {
        match self.overworld().generator {
            LevelGenerator::Debug(_) => SpecialWorldProperty::Debug,
            LevelGenerator::Flat(_) => SpecialWorldProperty::Flat,
            LevelGenerator::Noise(_) => SpecialWorldProperty::None,
        }
    }

    /// `withOverworld(dimensions, type, generator)`: replaces (or adds) the overworld stem.
    pub fn with_overworld(&self, dimension_type: DimensionTypeRef, generator: LevelGenerator) -> WorldDimensions {
        let stem = Arc::new(LevelStem { dimension_type, generator });
        let mut dimensions = self.dimensions.clone();
        match dimensions.iter_mut().find(|(k, _)| *k == overworld_key()) {
            Some(entry) => entry.1 = stem,
            None => dimensions.push((overworld_key(), stem)),
        }
        WorldDimensions { dimensions }
    }
}

// ---- WorldPreset ------------------------------------------------------------------------------------

/// `WorldPreset`: dimension stems keyed by dimension id.
pub struct WorldPreset {
    dimensions: Vec<(Identifier, Arc<LevelStem>)>,
}

impl WorldPreset {
    /// `WorldPreset.DIRECT_CODEC` (with `requireOverworld`).
    pub fn from_json(
        value: &Value,
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
    ) -> LoadResult<WorldPreset> {
        let fields = Fields::new(value)?;
        let dimensions = fields.required("dimensions", |v| {
            let map: &Map<String, Value> = v.as_object().ok_or_else(|| LoadError::new("expected object"))?;
            map.iter()
                .map(|(key, stem)| {
                    let id = Identifier::read(key).map_err(|e| LoadError::new(e).at(key))?;
                    let stem = LevelStem::from_json(stem, registries, biomes).map_err(|e| e.at(key))?;
                    Ok((id, Arc::new(stem)))
                })
                .collect::<LoadResult<Vec<_>>>()
        })?;
        if !dimensions.iter().any(|(k, _)| *k == overworld_key()) {
            return Err(LoadError::new("Missing overworld dimension"));
        }
        Ok(WorldPreset { dimensions })
    }

    /// `createWorldDimensions` (`dimensionsInOrder`).
    pub fn create_world_dimensions(&self) -> WorldDimensions {
        let keys: Vec<Identifier> = self.dimensions.iter().map(|(k, _)| k.clone()).collect();
        let ordered = keys_in_order(&keys)
            .into_iter()
            .filter_map(|k| self.dimensions.iter().find(|(d, _)| *d == k).map(|(_, v)| (k, v.clone())))
            .collect();
        WorldDimensions { dimensions: ordered }
    }

    /// `overworld()`.
    pub fn overworld(&self) -> Option<&Arc<LevelStem>> {
        self.dimensions.iter().find(|(k, _)| *k == overworld_key()).map(|(_, v)| v)
    }
}

/// `WorldPresets`: keys and the helpers working on registered presets.
pub mod world_presets {
    use super::*;

    pub const NORMAL: &str = "normal";
    pub const FLAT: &str = "flat";
    pub const FLAT_ALL_DIMENSIONS: &str = "flat_all_dimensions";
    pub const LARGE_BIOMES: &str = "large_biomes";
    pub const AMPLIFIED: &str = "amplified";
    pub const SINGLE_BIOME_SURFACE: &str = "single_biome_surface";
    pub const DEBUG: &str = "debug_all_block_states";

    /// The registered preset `id` (shared like a holder reference).
    pub fn load(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        id: &Identifier,
    ) -> LoadResult<Arc<WorldPreset>> {
        registries.memoized(WORLD_PRESET_REGISTRY, id, |json, _| WorldPreset::from_json(json, registries, biomes))
    }

    /// `fromSettings`: the preset key the overworld generator maps back to.
    pub fn from_settings(dimensions: &WorldDimensions) -> Option<Identifier> {
        let overworld = dimensions.get(&overworld_key())?;
        Some(minecraft(match overworld.generator {
            LevelGenerator::Flat(_) => FLAT,
            LevelGenerator::Debug(_) => DEBUG,
            LevelGenerator::Noise(_) => NORMAL,
        }))
    }

    /// `createNormalWorldDimensions`.
    pub fn create_normal_world_dimensions(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
    ) -> LoadResult<WorldDimensions> {
        Ok(load(registries, biomes, &minecraft(NORMAL))?.create_world_dimensions())
    }

    /// `getNormalOverworld`.
    pub fn get_normal_overworld(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
    ) -> LoadResult<Arc<LevelStem>> {
        load(registries, biomes, &minecraft(NORMAL))?
            .overworld()
            .cloned()
            .ok_or_else(|| LoadError::new("Missing overworld dimension"))
    }

    /// `createTestWorldDimensions`.
    pub fn create_test_world_dimensions(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
    ) -> LoadResult<WorldDimensions> {
        Ok(load(registries, biomes, &minecraft(FLAT_ALL_DIMENSIONS))?.create_world_dimensions())
    }
}

/// `flatSettingsForBiomeAndLayers` of the `flat_all_dimensions` bootstrap, for callers building the same
/// settings programmatically.
pub fn flat_settings_for_biome_and_layers(
    registries: &WorldgenRegistries,
    biomes: &BiomeRegistry,
    biome: &Identifier,
    layers: &[FlatLayerInfo],
) -> LoadResult<FlatLevelGeneratorSettings> {
    let biome = biomes
        .id_of(biome)
        .ok_or_else(|| LoadError::new(format!("Missing element {biome} in minecraft:worldgen/biome")))?;
    Ok(FlatLevelGeneratorSettings::get_default(registries, biomes)?.with_biome_and_layers(layers, None, biome))
}
