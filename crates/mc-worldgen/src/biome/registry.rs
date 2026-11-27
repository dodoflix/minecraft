//! The biome registry: `Biome` (climate settings + generation settings), `BiomeGenerationSettings` and the
//! registry holding every `worldgen/biome` entry in vanilla's (dedicated server) id order.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::Value;

use mc_core::ident::Identifier;
use mc_core::pos::block_pos::BlockPos;
use mc_core::random::{LegacyRandomSource, WorldgenRandom};
use mc_data::block::BlockState;
use mc_world::palette::BiomeId;

use crate::carver::{WorldCarver, carver_from_json};
use crate::data::{LoadError, WorldgenDataSource, WorldgenRegistries};
use crate::densityfunction::codec::{
    Fields, LoadResult, parse_bool, parse_f32, parse_identifier, parse_list, parse_str,
};
use crate::synth::{Noise, NoiseStack, SimplexNoise};

/// The registry folder of biomes.
pub const BIOME_REGISTRY: &str = "worldgen/biome";
const CARVER_TAG_DIR: &str = "worldgen/carver";
const PLACED_FEATURE_TAG_DIR: &str = "worldgen/placed_feature";
const BIOME_TAG_DIR: &str = "worldgen/biome";

static TEMPERATURE_NOISE: LazyLock<SimplexNoise> = LazyLock::new(|| {
    SimplexNoise::new(
        &mut WorldgenRandom::new(Box::new(LegacyRandomSource::new(1234))),
        true,
    )
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
    SimplexNoise::new(
        &mut WorldgenRandom::new(Box::new(LegacyRandomSource::new(2345))),
        true,
    )
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
                let ground_value_edge_variation =
                    f64::from(BIOME_INFO_NOISE.get2(x * 0.2, z * 0.2));
                let ice_patches = ground_value_large_variation + ground_value_edge_variation;
                if ice_patches < 0.3 {
                    let ground_value_small_variation =
                        f64::from(BIOME_INFO_NOISE.get2(x * 0.09, z * 0.09));
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

/// Identity source of [`InlineFeature`].
static NEXT_INLINE_FEATURE: AtomicU64 = AtomicU64::new(0);

/// An inline `PlacedFeature` definition of a biome's feature steps (`Holder.Direct`), decoded by the game crates (see
/// [`INLINE_PLACED_FEATURE_DECODERS`]). A decode is its own identity: equal JSON read twice gives two features.
#[derive(Clone)]
pub struct InlineFeature {
    id: u64,
    json: Arc<Value>,
    decoded: Arc<dyn Any + Send + Sync>,
}

impl InlineFeature {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// What the registered decoder returned for the definition.
    pub fn decoded(&self) -> &Arc<dyn Any + Send + Sync> {
        &self.decoded
    }
}

impl PartialEq for InlineFeature {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for InlineFeature {}

impl Hash for InlineFeature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl fmt::Debug for InlineFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InlineFeature")
            .field("id", &self.id)
            .field("json", &self.json)
            .finish()
    }
}

/// A placed feature of a biome's feature steps: a registry reference, an inline definition, or the inline fill-layer
/// feature flat worlds add (`FlatLevelGeneratorSettings.adjustGenerationSettings`).
#[derive(Clone, Debug, PartialEq)]
pub enum PlacedFeatureRef {
    Named(Identifier),
    InlineFillLayer { height: i32, state: BlockState },
    Inline(InlineFeature),
}

impl PlacedFeatureRef {
    /// Equality of what was read, not of identity: two inline features are the same definition when their JSON is.
    fn same_definition(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Inline(a), Self::Inline(b)) => a.json == b.json,
            _ => self == other,
        }
    }
}

/// An inline `WorldCarver` definition of a biome's carvers (`Holder.Direct`).
#[derive(Clone)]
pub struct InlineCarver {
    json: Arc<Value>,
    carver: Arc<dyn WorldCarver>,
}

impl InlineCarver {
    pub fn carver(&self) -> &Arc<dyn WorldCarver> {
        &self.carver
    }
}

impl fmt::Debug for InlineCarver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("InlineCarver").field(&self.json).finish()
    }
}

/// A carver of a biome's carver list: a registry reference or an inline definition.
#[derive(Clone, Debug)]
pub enum CarverRef {
    Named(Identifier),
    Inline(InlineCarver),
}

impl PartialEq for CarverRef {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Named(a), Self::Named(b)) => a == b,
            (Self::Inline(a), Self::Inline(b)) => Arc::ptr_eq(&a.carver, &b.carver),
            _ => false,
        }
    }
}

impl CarverRef {
    fn same_definition(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Inline(a), Self::Inline(b)) => a.json == b.json,
            _ => self == other,
        }
    }
}

/// `BiomeGenerationSettings`: carvers (flattened `HolderSet<WorldCarver>`) and the features of every
/// `GenerationStep.Decoration`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BiomeGenerationSettings {
    pub carvers: Vec<CarverRef>,
    pub features: Vec<Vec<PlacedFeatureRef>>,
}

/// An element of a `HolderSet` of the generation settings: a registry id or an inline (`Holder.Direct`) definition.
trait Element: Sized {
    /// The registry its ids and `#tag`s refer to.
    const REGISTRY: &'static str;
    fn named(id: Identifier) -> Self;
    fn inline(value: &Value, biomes: &BiomeRegistry) -> LoadResult<Self>;
}

impl Element for CarverRef {
    const REGISTRY: &'static str = CARVER_TAG_DIR;

    fn named(id: Identifier) -> Self {
        CarverRef::Named(id)
    }

    fn inline(value: &Value, _biomes: &BiomeRegistry) -> LoadResult<Self> {
        Ok(CarverRef::Inline(InlineCarver {
            json: Arc::new(value.clone()),
            carver: carver_from_json(value)?,
        }))
    }
}

impl Element for PlacedFeatureRef {
    const REGISTRY: &'static str = PLACED_FEATURE_TAG_DIR;

    fn named(id: Identifier) -> Self {
        PlacedFeatureRef::Named(id)
    }

    fn inline(value: &Value, biomes: &BiomeRegistry) -> LoadResult<Self> {
        let decode = INLINE_PLACED_FEATURE_DECODERS
            .first()
            .ok_or_else(|| LoadError::new("No decoder for inline placed features is linked"))?;
        // shortcut: the decode starts from fresh registries (the decoder re-binds every feature holder), upgrade when an
        // inline feature is read on a hot path
        let worldgen = WorldgenRegistries::new(biomes.source.clone());
        Ok(PlacedFeatureRef::Inline(InlineFeature {
            id: NEXT_INLINE_FEATURE.fetch_add(1, Ordering::Relaxed),
            json: Arc::new(value.clone()),
            decoded: decode(value, biomes, &worldgen)?,
        }))
    }
}

impl BiomeGenerationSettings {
    /// `BiomeGenerationSettings.EMPTY` is `BiomeGenerationSettings::default()`.
    pub fn new(
        carvers: Vec<CarverRef>,
        features: Vec<Vec<PlacedFeatureRef>>,
    ) -> BiomeGenerationSettings {
        BiomeGenerationSettings { carvers, features }
    }

    /// `getCarvers()`.
    pub fn get_carvers(&self) -> &[CarverRef] {
        &self.carvers
    }

    /// `features()`.
    pub fn features(&self) -> &[Vec<PlacedFeatureRef>] {
        &self.features
    }

    /// Whether both read the same definitions (inline entries by their JSON instead of their identity).
    fn same_definition(&self, other: &Self) -> bool {
        self.carvers.len() == other.carvers.len()
            && self.features.len() == other.features.len()
            && self
                .carvers
                .iter()
                .zip(&other.carvers)
                .all(|(a, b)| a.same_definition(b))
            && self.features.iter().zip(&other.features).all(|(a, b)| {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.same_definition(b))
            })
    }

    /// `BiomeGenerationSettings.CODEC` of a registry entry (carvers: id | `#tag` | list of ids; features: a list of lists of
    /// ids). Inline definitions are not supported here. As `promotePartial` keeps the partial result of the features, a
    /// step that is no list (a lone id) is dropped.
    fn from_json(biome: &Value) -> LoadResult<BiomeGenerationSettings> {
        let fields = Fields::new(biome)?;
        let carvers = fields.required("carvers", |v| holder_set(v, false))?;
        let features = fields.required("features", |v| {
            let Value::Array(steps) = v else {
                return Err(LoadError::new("Not a list"));
            };
            Ok(steps
                .iter()
                .filter_map(|step| holder_set(step, true).ok())
                .collect())
        })?;
        Ok(BiomeGenerationSettings { carvers, features })
    }

    /// `BiomeGenerationSettings.CODEC` of an inline biome: `promotePartial` turns the partial result of a list whose
    /// entries fail into its result, so an id that does not resolve (or is no id at all) and an inline definition that
    /// does not decode are dropped from their list, a feature step that fails as a whole is dropped from the steps, and
    /// a carvers field that fails as a whole fails the biome.
    fn from_json_inline(
        biome: &Value,
        biomes: &BiomeRegistry,
    ) -> LoadResult<BiomeGenerationSettings> {
        let fields = Fields::new(biome)?;
        let carvers = fields.required("carvers", |v| {
            partial_holder_set(v, false, biomes)
                .ok_or_else(|| LoadError::new("Failed to get element from registry"))
        })?;
        let features = fields.required("features", |v| {
            let Value::Array(steps) = v else {
                return Err(LoadError::new("Not a list"));
            };
            Ok(steps
                .iter()
                .filter_map(|step| partial_holder_set(step, true, biomes))
                .collect())
        })?;
        Ok(BiomeGenerationSettings { carvers, features })
    }
}

/// A `HolderSet` of registry ids by id, `#tag` (resolved against the vanilla tags of the registry) or list of ids
/// (`HolderSetCodec`); unless `always_list`, a lone id is a list of one.
fn holder_set<E: Element>(value: &Value, always_list: bool) -> LoadResult<Vec<E>> {
    match value {
        Value::String(s) => {
            if let Some(tag) = s.strip_prefix('#') {
                let tag = Identifier::read(tag).map_err(LoadError::new)?;
                let names =
                    mc_data::tags::names(E::REGISTRY, &tag.to_string()).ok_or_else(|| {
                        LoadError::new(format!(
                            "Missing tag: '{tag}' in 'minecraft:{}'",
                            E::REGISTRY
                        ))
                    })?;
                names
                    .iter()
                    .map(|n| Identifier::read(n).map(E::named).map_err(LoadError::new))
                    .collect()
            } else if always_list {
                Err(LoadError::new("Not a list"))
            } else {
                Ok(vec![E::named(parse_identifier(value)?)])
            }
        }
        Value::Array(_) => parse_list(value, |e| match e {
            Value::String(_) => Ok(E::named(parse_identifier(e)?)),
            _ => Err(LoadError::new(
                "inline registry entries are not supported here",
            )),
        }),
        _ => Err(LoadError::new("expected id, tag or list")),
    }
}

/// [`holder_set`] as a list codec keeps what decodes: `None` when the set fails as a whole (an unresolved tag or id, a
/// value that is no set), else the entries that resolve in the registry's data source or decode as inline definitions. A
/// lone inline definition is a list of one (`ExtraCodecs.compactListCodec`).
fn partial_holder_set<E: Element>(
    value: &Value,
    always_list: bool,
    biomes: &BiomeRegistry,
) -> Option<Vec<E>> {
    let resolved = |value: &Value| match value {
        Value::String(s) => Identifier::read(s)
            .ok()
            .filter(|id| biomes.source.get(E::REGISTRY, id).is_some())
            .map(E::named),
        Value::Object(_) => E::inline(value, biomes).ok(),
        _ => None,
    };
    match value {
        Value::String(s) if s.starts_with('#') => holder_set(value, always_list).ok(),
        // `HolderSetCodec` with `alwaysUseList`: a lone id is no set
        Value::String(_) if always_list => None,
        Value::String(_) => resolved(value).map(|entry| vec![entry]),
        Value::Array(items) => Some(items.iter().filter_map(resolved).collect()),
        Value::Object(_) if !always_list => resolved(value).map(|entry| vec![entry]),
        _ => None,
    }
}

/// Decoder of an inline `PlacedFeature` (`PlacedFeature.DIRECT_CODEC`, its holder references bound to the registries of
/// the data source), returning the decoded feature. The codecs live in the game crates that depend on this one: one of
/// them registers itself here. Without one an inline placed feature fails to decode.
pub type InlinePlacedFeatureDecoder =
    fn(&Value, &BiomeRegistry, &WorldgenRegistries) -> LoadResult<Arc<dyn Any + Send + Sync>>;

#[linkme::distributed_slice]
pub static INLINE_PLACED_FEATURE_DECODERS: [InlinePlacedFeatureDecoder];

/// Checks of the `attributes` of a biome (`EnvironmentAttributeMap.CODEC_ONLY_POSITIONAL`), whose codecs live in the game
/// crates that depend on this one: they register themselves here. An inline biome with `attributes` fails to decode while
/// none is registered; the entries of the registry are trusted data.
#[linkme::distributed_slice]
pub static BIOME_ATTRIBUTE_VALIDATORS: [fn(&Value) -> Result<(), String>];

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
        Biome::decode(value, false, BiomeGenerationSettings::from_json)
    }

    /// `Biome.DIRECT_CODEC` of an inline definition, with the carver and placed feature entries resolved against the data
    /// source of `biomes` (see [`BiomeGenerationSettings::from_json_inline`]).
    fn from_json_inline(value: &Value, biomes: &BiomeRegistry) -> LoadResult<Biome> {
        Biome::decode(value, true, |v| {
            BiomeGenerationSettings::from_json_inline(v, biomes)
        })
    }

    fn decode(
        value: &Value,
        inline: bool,
        generation_settings: impl FnOnce(&Value) -> LoadResult<BiomeGenerationSettings>,
    ) -> LoadResult<Biome> {
        let fields = Fields::new(value)?;
        let climate = ClimateSettings::from_json(value)?;
        let attributes = fields.optional("attributes", |v| {
            if inline && BIOME_ATTRIBUTE_VALIDATORS.is_empty() {
                return Err(LoadError::new("No decoder for biome attributes is linked"));
            }
            for validate in BIOME_ATTRIBUTE_VALIDATORS {
                validate(v).map_err(LoadError::new)?;
            }
            Ok(v.clone())
        })?;
        let effects = fields.required("effects", |v| Ok(v.clone()))?;
        let generation_settings = generation_settings(value)?;
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

    /// `getHeightAdjustedTemperature(pos, seaLevel)` (private in vanilla; public for the differential tests).
    pub fn get_height_adjusted_temperature(&self, pos: BlockPos, sea_level: i32) -> f32 {
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
            adjusted_temperature - (v + pos.y() as f32 - snow_level as f32) * 0.05f32 / 40.0f32
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

/// `BiomeId`s of inline (`Holder.Direct`) biomes start here: they are no registry entries, so they never collide
/// with registry ids (vanilla's `IdMap` gives a direct holder the id -1 and cannot serialize it by name either).
const INLINE_BIOME_BASE: u32 = 1 << 30;

/// Every inline biome definition decoded so far, deduplicated by its JSON and what it read (the entries of the generation
/// settings resolve against the data source, which differs between registries). Direct holders live as long as the
/// generator/levels using them, so the entries are leaked (`&'static`) and ids stay valid across registry instances.
static INLINE_BIOMES: Mutex<Vec<(Value, &'static Biome)>> = Mutex::new(Vec::new());

/// What [`BiomeRegistry::key`] reports for an inline biome: never a registry entry, so no tag or id check matches it.
static INLINE_BIOME_KEY: LazyLock<Identifier> =
    LazyLock::new(|| Identifier::with_default_namespace("__inline_biome__").expect("valid id"));

/// `Biome.DIRECT_CODEC` as a `Holder.Direct`: the id of the (deduplicated) inline definition.
fn inline_biome_id(value: &Value, biomes: &BiomeRegistry) -> LoadResult<BiomeId> {
    let biome = Biome::from_json_inline(value, biomes)?;
    let mut inline = INLINE_BIOMES.lock().unwrap_or_else(|e| e.into_inner());
    let index = match inline.iter().position(|(json, b)| {
        json == value
            && b.generation_settings
                .same_definition(&biome.generation_settings)
    }) {
        Some(index) => index,
        None => {
            inline.push((value.clone(), Box::leak(Box::new(biome))));
            inline.len() - 1
        }
    };
    Ok(BiomeId(INLINE_BIOME_BASE + index as u32))
}

fn inline_biome(id: BiomeId) -> &'static Biome {
    let inline = INLINE_BIOMES.lock().unwrap_or_else(|e| e.into_inner());
    inline[(id.0 - INLINE_BIOME_BASE) as usize].1
}

/// The biome registry as the dedicated server builds it from the vanilla data pack: entries are
/// registered in `worldgen/biome` resource-listing order, which is the byte order of the identifiers, so
/// `BiomeId(i)` is the i-th identifier in sorted order (67 entries in 26.3, `minecraft:badlands` = 0 ...
/// `minecraft:wooded_badlands` = 66).
pub struct BiomeRegistry {
    biomes: Vec<Biome>,
    keys: Vec<Identifier>,
    ids: HashMap<Identifier, BiomeId>,
    /// The data the biomes were loaded from: carver / placed feature references of inline biomes resolve against it.
    source: Arc<dyn WorldgenDataSource>,
}

impl std::fmt::Debug for BiomeRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiomeRegistry")
            .field("keys", &self.keys)
            .finish()
    }
}

impl BiomeRegistry {
    /// Loads every `worldgen/biome` entry of the data source (in sorted identifier order).
    pub fn load(registries: &WorldgenRegistries) -> LoadResult<BiomeRegistry> {
        let mut keys = registries.list(BIOME_REGISTRY);
        keys.sort_by_key(|key| key.to_string());
        let mut biomes = Vec::with_capacity(keys.len());
        for key in &keys {
            let json = registries.raw(BIOME_REGISTRY, key).ok_or_else(|| {
                LoadError::new("no such registry entry").in_entry(BIOME_REGISTRY, key)
            })?;
            biomes.push(Biome::from_json(&json).map_err(|e| e.in_entry(BIOME_REGISTRY, key))?);
        }
        let ids = keys
            .iter()
            .enumerate()
            .map(|(i, key)| (key.clone(), BiomeId(i as u32)))
            .collect();
        Ok(BiomeRegistry {
            biomes,
            keys,
            ids,
            source: registries.source().clone(),
        })
    }

    pub fn len(&self) -> usize {
        self.biomes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.biomes.is_empty()
    }

    /// Whether `id` is an inline (`Holder.Direct`) biome instead of a registry entry.
    pub fn is_inline(id: BiomeId) -> bool {
        id.0 >= INLINE_BIOME_BASE
    }

    /// `getId(key)` as an id.
    pub fn id_of(&self, key: &Identifier) -> Option<BiomeId> {
        self.ids.get(key).copied()
    }

    /// `getId` by a path in the `minecraft` namespace (`"plains"`).
    pub fn id_of_name(&self, name: &str) -> Option<BiomeId> {
        self.id_of(&Identifier::try_parse(name)?)
    }

    /// `getKey(biome)`; an inline biome has no key and reports a name no registry entry has. Panics on an id outside
    /// the registry.
    pub fn key(&self, id: BiomeId) -> &Identifier {
        if Self::is_inline(id) {
            &INLINE_BIOME_KEY
        } else {
            &self.keys[id.0 as usize]
        }
    }

    /// `byId(id)`. Panics on an id outside the registry.
    pub fn get(&self, id: BiomeId) -> &Biome {
        if Self::is_inline(id) {
            inline_biome(id)
        } else {
            &self.biomes[id.0 as usize]
        }
    }

    /// All ids in registry order.
    pub fn ids(&self) -> impl Iterator<Item = BiomeId> + use<> {
        (0..self.biomes.len() as u32).map(BiomeId)
    }

    /// The registry names in id order (for `mc_world::palette::BiomeRegistry::new`).
    pub fn names(&self) -> Vec<String> {
        self.keys.iter().map(|k| k.to_string()).collect()
    }

    /// `Biome.CODEC` (`RegistryFileCodec`): a registered biome id, or an inline `Biome.DIRECT_CODEC` definition (any
    /// non-string value, like vanilla's "not a resource key" fallback). The entries of an inline definition's generation
    /// settings that fail are dropped (`promotePartial`), also in the lenient flat `biome` field.
    pub fn holder_from_json(&self, value: &Value) -> LoadResult<BiomeId> {
        if !value.is_string() {
            return inline_biome_id(value, self);
        }
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
                let names =
                    mc_data::tags::names(BIOME_TAG_DIR, &tag.to_string()).ok_or_else(|| {
                        LoadError::new(format!(
                            "Missing tag: '{tag}' in 'minecraft:worldgen/biome'"
                        ))
                    })?;
                names
                    .iter()
                    .map(|n| {
                        let id = Identifier::read(n).map_err(LoadError::new)?;
                        self.id_of(&id)
                            .ok_or_else(|| LoadError::new(format!("Failed to get element {id}")))
                    })
                    .collect()
            }
            Value::Array(_) => parse_list(value, |e| self.holder_from_json(e)),
            // `ExtraCodecs.compactListCodec`: a lone holder (id or inline definition) is a list of one
            _ => Ok(vec![self.holder_from_json(value)?]),
        }
    }
}
