//! `LevelStem`, `WorldPreset`/`WorldPresets`, `WorldDimensions` and the dimension `generator` factory
//! (`ChunkGenerator.CODEC`: `minecraft:noise`, `minecraft:flat`, `minecraft:debug`).

use std::sync::Arc;

use mc_core::ident::Identifier;
use serde_json::{Map, Value};

use crate::biome::{BiomeRegistry, BiomeSource, FixedBiomeSource};
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
    pub generator: Arc<NoiseBasedChunkGenerator>,
    /// `type` of the biome source codec, e.g. `minecraft:multi_noise`.
    pub biome_source_type: Identifier,
    /// The registered `MultiNoiseBiomeSourceParameterList` a `minecraft:multi_noise` source references (`None` for a
    /// parameter list given inline or another source type): `MultiNoiseBiomeSource.stable`.
    pub biome_source_preset: Option<Identifier>,
    /// The `settings` reference (`None` for inline settings).
    pub settings: Option<Identifier>,
}

impl NoiseGenerator {
    /// `NoiseBasedChunkGenerator.stable(settings)`: uses the registered noise settings `key`.
    pub fn stable(&self, key: &Identifier) -> bool {
        self.settings.as_ref() == Some(key)
    }
}

/// The three vanilla `ChunkGenerator` implementations a dimension can use.
pub enum LevelGenerator {
    Noise(NoiseGenerator),
    Flat(Arc<FlatLevelSource>),
    Debug(Arc<DebugLevelSource>),
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
            LevelGenerator::Noise(n) => n.generator.as_ref(),
            LevelGenerator::Flat(f) => f.as_ref(),
            LevelGenerator::Debug(d) => d.as_ref(),
        }
    }

    /// The biome source as a shareable handle (flat and debug generators use a fixed biome source).
    pub fn biome_source(&self) -> Arc<dyn BiomeSource> {
        match self {
            LevelGenerator::Noise(n) => n.generator.biome_source_arc().clone(),
            _ => Arc::new(FixedBiomeSource::new(self.as_chunk_generator().biome_source().possible_biomes()[0])),
        }
    }

    /// The generator shared with whoever creates the level from this stem.
    pub fn chunk_generator(&self) -> Arc<dyn ChunkGenerator> {
        match self {
            LevelGenerator::Noise(n) => n.generator.clone(),
            LevelGenerator::Flat(f) => f.clone(),
            LevelGenerator::Debug(d) => d.clone(),
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
            let biome_source_preset = (biome_source_type == minecraft("multi_noise") && biome_source_json.get("biomes").is_none())
                .then(|| biome_source_json.get("preset").and_then(Value::as_str).and_then(Identifier::try_parse))
                .flatten();
            Ok(LevelGenerator::Noise(NoiseGenerator { generator: Arc::new(generator), biome_source_type, biome_source_preset, settings: settings_id }))
        }
        "minecraft:flat" => {
            let settings = fields.required("settings", |v| FlatLevelGeneratorSettings::from_json(v, registries, biomes))?;
            Ok(LevelGenerator::Flat(Arc::new(FlatLevelSource::new(settings, biomes.clone()))))
        }
        "minecraft:debug" => Ok(LevelGenerator::Debug(Arc::new(DebugLevelSource::new(get_default_biome(biomes)?, biomes.clone())))),
        other => Err(LoadError::new(format!("Unknown registry key in minecraft:chunk_generator: {other}")).at("type")),
    }
}

type Stems<D> = Vec<(Identifier, Arc<LevelStem<D>>)>;

// ---- level stems ------------------------------------------------------------------------------------

/// `Holder<T>`: a registry reference (key + value) or a `Holder.Direct` inline value.
#[derive(Debug)]
pub enum Holder<T> {
    Reference(Identifier, Arc<T>),
    Direct(Arc<T>),
}

impl<T> Clone for Holder<T> {
    fn clone(&self) -> Self {
        match self {
            Holder::Reference(key, value) => Holder::Reference(key.clone(), value.clone()),
            Holder::Direct(value) => Holder::Direct(value.clone()),
        }
    }
}

impl<T> Holder<T> {
    pub fn value(&self) -> &Arc<T> {
        match self {
            Holder::Reference(_, value) | Holder::Direct(value) => value,
        }
    }

    /// `unwrapKey`.
    pub fn key(&self) -> Option<&Identifier> {
        match self {
            Holder::Reference(key, _) => Some(key),
            Holder::Direct(_) => None,
        }
    }

    /// `is(ResourceKey)`: a reference to exactly that key.
    pub fn is(&self, key: &Identifier) -> bool {
        self.key() == Some(key)
    }
}

/// `DimensionType.CODEC` as `LevelStem` needs it: the `dimension_type` registry and `DimensionType.DIRECT_CODEC`
/// (the type itself lives in the game crate, above this one).
pub trait DimensionTypes {
    type Value: Send + Sync + 'static;

    /// The registered entry `id`, or the lookup error.
    fn reference(&self, id: &Identifier) -> LoadResult<Arc<Self::Value>>;

    /// An inline definition decoded (and validated) by `DimensionType.DIRECT_CODEC`.
    fn direct(&self, json: &Value) -> LoadResult<Arc<Self::Value>>;
}

/// `DimensionType.CODEC` = `RegistryFileCodec`: a string is a registry reference, anything else an inline definition.
pub fn dimension_type_holder_from_json<D: DimensionTypes>(value: &Value, dimension_types: &D) -> LoadResult<Holder<D::Value>> {
    match value {
        Value::String(_) => {
            let id = parse_identifier(value)?;
            Ok(Holder::Reference(id.clone(), dimension_types.reference(&id)?))
        }
        _ => Ok(Holder::Direct(dimension_types.direct(value)?)),
    }
}

/// `LevelStem`.
pub struct LevelStem<D> {
    pub dimension_type: Holder<D>,
    pub generator: LevelGenerator,
}

impl<D> LevelStem<D> {
    /// `LevelStem.CODEC`.
    pub fn from_json<T: DimensionTypes<Value = D>>(
        value: &Value,
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        dimension_types: &T,
    ) -> LoadResult<LevelStem<D>> {
        let fields = Fields::new(value)?;
        let dimension_type = fields.required("type", |v| dimension_type_holder_from_json(v, dimension_types))?;
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

/// DFU `Lifecycle` as far as registries use it: `stable`, or `experimental` (which absorbs everything).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Stable,
    Experimental,
}

impl Lifecycle {
    /// `Lifecycle.add`.
    pub fn add(self, other: Lifecycle) -> Lifecycle {
        if self == Lifecycle::Experimental || other == Lifecycle::Experimental { Lifecycle::Experimental } else { Lifecycle::Stable }
    }
}

/// `WorldDimensions`: dimension key -> stem, with a mandatory overworld.
pub struct WorldDimensions<D> {
    dimensions: Vec<(Identifier, Arc<LevelStem<D>>)>,
}

impl<D> Clone for WorldDimensions<D> {
    fn clone(&self) -> Self {
        WorldDimensions { dimensions: self.dimensions.clone() }
    }
}

/// One entry of the registry `WorldDimensions.bake` produces: the stem and its `RegistrationInfo` stability (the
/// registration never carries a known pack).
pub struct BakedDimension<D> {
    pub key: Identifier,
    pub stem: Arc<LevelStem<D>>,
    pub lifecycle: Lifecycle,
}

/// `WorldDimensions.Complete`: the baked `dimension` registry (in registration order) and what is derived from it.
pub struct Complete<D> {
    pub dimensions: Vec<BakedDimension<D>>,
    pub special_world_property: SpecialWorldProperty,
    /// The registry's lifecycle (`registryLifecycle`).
    pub lifecycle: Lifecycle,
}

fn special_property_of(generator: &LevelGenerator) -> SpecialWorldProperty {
    match generator {
        LevelGenerator::Debug(_) => SpecialWorldProperty::Debug,
        LevelGenerator::Flat(_) => SpecialWorldProperty::Flat,
        LevelGenerator::Noise(_) => SpecialWorldProperty::None,
    }
}

impl<D> WorldDimensions<D> {
    /// `new WorldDimensions(map)`: "Overworld settings missing" without an overworld.
    pub fn new(dimensions: Vec<(Identifier, Arc<LevelStem<D>>)>) -> Result<WorldDimensions<D>, LoadError> {
        if !dimensions.iter().any(|(k, _)| *k == overworld_key()) {
            return Err(LoadError::new("Overworld settings missing"));
        }
        Ok(WorldDimensions { dimensions })
    }

    /// `WorldDimensions.CODEC`: `{"dimensions": {<dimension id>: <level stem>}}` (`unboundedMap`).
    pub fn from_json<T: DimensionTypes<Value = D>>(
        value: &Value,
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        dimension_types: &T,
    ) -> LoadResult<WorldDimensions<D>> {
        let fields = Fields::new(value)?;
        // The record codec builds `WorldDimensions` from the partial map of a failed `unboundedMap`, so a failing
        // overworld stem surfaces as the constructor's "Overworld settings missing", not as its own error.
        let (dimensions, error) = fields.required("dimensions", |v| stems_from_json_partial(v, registries, biomes, dimension_types))?;
        let world = WorldDimensions::new(dimensions)?;
        match error {
            Some(error) => Err(error),
            None => Ok(world),
        }
    }

    pub fn dimensions(&self) -> &[(Identifier, Arc<LevelStem<D>>)] {
        &self.dimensions
    }

    pub fn get(&self, key: &Identifier) -> Option<&Arc<LevelStem<D>>> {
        self.dimensions.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn overworld(&self) -> &Arc<LevelStem<D>> {
        self.get(&overworld_key()).expect("checked at construction")
    }

    /// `levels()`: the level keys (`Registries.levelStemToLevel` keeps the id) in dimension order.
    pub fn levels(&self) -> Vec<Identifier> {
        self.dimensions.iter().map(|(k, _)| k.clone()).collect()
    }

    /// `isDebug`.
    pub fn is_debug(&self) -> bool {
        matches!(self.overworld().generator, LevelGenerator::Debug(_))
    }

    /// `specialWorldProperty`.
    pub fn special_world_property(&self) -> SpecialWorldProperty {
        special_property_of(&self.overworld().generator)
    }

    /// `withOverworld(dimensions, type, generator)`: replaces (or adds) the overworld stem.
    pub fn with_overworld(&self, dimension_type: Holder<D>, generator: LevelGenerator) -> WorldDimensions<D> {
        let stem = Arc::new(LevelStem { dimension_type, generator });
        let mut dimensions = self.dimensions.clone();
        match dimensions.iter_mut().find(|(k, _)| *k == overworld_key()) {
            Some(entry) => entry.1 = stem,
            None => dimensions.push((overworld_key(), stem)),
        }
        WorldDimensions { dimensions }
    }

    /// `replaceOverworldGenerator(registries, generator)`: the overworld keeps its dimension type (the registered
    /// `minecraft:overworld` type from `default_type` when there is no overworld stem).
    pub fn replace_overworld_generator(
        &self,
        generator: LevelGenerator,
        default_type: impl FnOnce() -> LoadResult<Holder<D>>,
    ) -> LoadResult<WorldDimensions<D>> {
        let dimension_type = match self.get(&overworld_key()) {
            Some(stem) => stem.dimension_type.clone(),
            None => default_type()?,
        };
        Ok(self.with_overworld(dimension_type, generator))
    }

    /// `bake(baseDimensions)`: for every known key (the base registry's, then this world's own), the base registry's
    /// stem if it has one, else this world's. Every entry is registered with a `stable` lifecycle iff it is
    /// vanilla-like (`isVanillaLike`), the registry starts `stable` only when all three builtin dimensions are known.
    pub fn bake(&self, base_dimensions: &[(Identifier, Arc<LevelStem<D>>)]) -> Complete<D> {
        let mut known: Vec<Identifier> = base_dimensions.iter().map(|(k, _)| k.clone()).collect();
        for (key, _) in &self.dimensions {
            if !known.contains(key) {
                known.push(key.clone());
            }
        }
        let mut registry_lifecycle =
            if [overworld_key(), nether_key(), end_key()].iter().all(|k| known.contains(k)) { Lifecycle::Stable } else { Lifecycle::Experimental };
        let mut dimensions = Vec::new();
        for key in keys_in_order(&known) {
            let stem = base_dimensions.iter().find(|(k, _)| *k == key).map(|(_, v)| v).or_else(|| self.get(&key));
            if let Some(stem) = stem {
                let lifecycle = if is_vanilla_like(&key, stem) { Lifecycle::Stable } else { Lifecycle::Experimental };
                registry_lifecycle = registry_lifecycle.add(lifecycle);
                dimensions.push(BakedDimension { key, stem: stem.clone(), lifecycle });
            }
        }
        let special_world_property = dimensions
            .iter()
            .find(|d| d.key == overworld_key())
            .map_or(SpecialWorldProperty::None, |d| special_property_of(&d.stem.generator));
        Complete { dimensions, special_world_property, lifecycle: registry_lifecycle }
    }
}

/// `Codec.unboundedMap(ResourceKey.codec(LEVEL_STEM), LevelStem.CODEC)`: the entries that decoded, and the first
/// error (DFU keeps decoding after a failing entry and returns the rest as the partial result).
fn stems_from_json_partial<D, T: DimensionTypes<Value = D>>(
    value: &Value,
    registries: &Arc<WorldgenRegistries>,
    biomes: &Arc<BiomeRegistry>,
    dimension_types: &T,
) -> Result<(Stems<D>, Option<LoadError>), LoadError> {
    let map: &Map<String, Value> = value.as_object().ok_or_else(|| LoadError::new("expected object"))?;
    let mut stems = Vec::new();
    let mut first_error = None;
    for (key, stem) in map {
        let decoded = Identifier::read(key)
            .map_err(|e| LoadError::new(e).at(key))
            .and_then(|id| Ok((id, LevelStem::from_json(stem, registries, biomes, dimension_types).map_err(|e| e.at(key))?)));
        match decoded {
            Ok((id, stem)) => stems.push((id, Arc::new(stem))),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    Ok((stems, first_error))
}

fn stems_from_json<D, T: DimensionTypes<Value = D>>(
    value: &Value,
    registries: &Arc<WorldgenRegistries>,
    biomes: &Arc<BiomeRegistry>,
    dimension_types: &T,
) -> LoadResult<Stems<D>> {
    match stems_from_json_partial(value, registries, biomes, dimension_types)? {
        (stems, None) => Ok(stems),
        (_, Some(error)) => Err(error),
    }
}

/// `WorldDimensions.isVanillaLike`: the three builtin dimensions with their builtin dimension type, noise settings and
/// biome source.
pub fn is_vanilla_like<D>(key: &Identifier, stem: &LevelStem<D>) -> bool {
    let noise = match &stem.generator {
        LevelGenerator::Noise(noise) => Some(noise),
        _ => None,
    };
    let multi_noise = |noise: &NoiseGenerator| noise.biome_source_type == minecraft("multi_noise");
    if *key == overworld_key() {
        // `isStableOverworld`: a multi-noise source must use the registered overworld preset; other sources pass
        (stem.dimension_type.is(&minecraft("overworld")) || stem.dimension_type.is(&minecraft("overworld_caves")))
            && !noise.is_some_and(|n| multi_noise(n) && n.biome_source_preset != Some(minecraft("overworld")))
    } else if *key == nether_key() {
        stem.dimension_type.is(&minecraft("the_nether"))
            && noise.is_some_and(|n| n.stable(&minecraft("nether")) && multi_noise(n) && n.biome_source_preset == Some(minecraft("nether")))
    } else if *key == end_key() {
        stem.dimension_type.is(&minecraft("the_end"))
            && noise.is_some_and(|n| n.stable(&minecraft("end")) && n.biome_source_type == minecraft("the_end"))
    } else {
        false
    }
}

// ---- WorldPreset ------------------------------------------------------------------------------------

/// `WorldPreset`: dimension stems keyed by dimension id.
pub struct WorldPreset<D> {
    dimensions: Vec<(Identifier, Arc<LevelStem<D>>)>,
}

impl<D> WorldPreset<D> {
    /// `WorldPreset.DIRECT_CODEC` (with `requireOverworld`).
    pub fn from_json<T: DimensionTypes<Value = D>>(
        value: &Value,
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        dimension_types: &T,
    ) -> LoadResult<WorldPreset<D>> {
        let fields = Fields::new(value)?;
        let dimensions = fields.required("dimensions", |v| stems_from_json(v, registries, biomes, dimension_types))?;
        if !dimensions.iter().any(|(k, _)| *k == overworld_key()) {
            return Err(LoadError::new("Missing overworld dimension"));
        }
        Ok(WorldPreset { dimensions })
    }

    /// `createWorldDimensions` (`dimensionsInOrder`).
    pub fn create_world_dimensions(&self) -> WorldDimensions<D> {
        let keys: Vec<Identifier> = self.dimensions.iter().map(|(k, _)| k.clone()).collect();
        let ordered = keys_in_order(&keys)
            .into_iter()
            .filter_map(|k| self.dimensions.iter().find(|(d, _)| *d == k).map(|(_, v)| (k, v.clone())))
            .collect();
        WorldDimensions { dimensions: ordered }
    }

    /// `overworld()`.
    pub fn overworld(&self) -> Option<&Arc<LevelStem<D>>> {
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
    pub fn load<T: DimensionTypes>(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        dimension_types: &T,
        id: &Identifier,
    ) -> LoadResult<Arc<WorldPreset<T::Value>>> {
        registries.memoized(WORLD_PRESET_REGISTRY, id, |json, _| WorldPreset::from_json(json, registries, biomes, dimension_types))
    }

    /// `fromSettings`: the preset key the overworld generator maps back to.
    pub fn from_settings<D>(dimensions: &WorldDimensions<D>) -> Option<Identifier> {
        let overworld = dimensions.get(&overworld_key())?;
        Some(minecraft(match overworld.generator {
            LevelGenerator::Flat(_) => FLAT,
            LevelGenerator::Debug(_) => DEBUG,
            LevelGenerator::Noise(_) => NORMAL,
        }))
    }

    /// `createNormalWorldDimensions`.
    pub fn create_normal_world_dimensions<T: DimensionTypes>(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        dimension_types: &T,
    ) -> LoadResult<WorldDimensions<T::Value>> {
        Ok(load(registries, biomes, dimension_types, &minecraft(NORMAL))?.create_world_dimensions())
    }

    /// `getNormalOverworld`.
    pub fn get_normal_overworld<T: DimensionTypes>(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        dimension_types: &T,
    ) -> LoadResult<Arc<LevelStem<T::Value>>> {
        load(registries, biomes, dimension_types, &minecraft(NORMAL))?
            .overworld()
            .cloned()
            .ok_or_else(|| LoadError::new("Missing overworld dimension"))
    }

    /// `createTestWorldDimensions`.
    pub fn create_test_world_dimensions<T: DimensionTypes>(
        registries: &Arc<WorldgenRegistries>,
        biomes: &Arc<BiomeRegistry>,
        dimension_types: &T,
    ) -> LoadResult<WorldDimensions<T::Value>> {
        Ok(load(registries, biomes, dimension_types, &minecraft(FLAT_ALL_DIMENSIONS))?.create_world_dimensions())
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
