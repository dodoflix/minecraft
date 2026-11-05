//! `world.level.levelgen.WorldDimensions` over the typed stems of `mc_worldgen::world_preset`: the dimensions a world is
//! created with (a `world_preset`, the stored `world_gen_settings.dat`, `level-type` + `generator-settings`), baked with the
//! data pack's own `dimension` entries into the final `dimension` registry (`WorldDimensions.bake`).
//!
//! A [`DimensionDecoder`] plays the `RegistryOps` of `datapackWorldRegistries`: it decodes `LevelStem.CODEC` twice, once
//! through the registry codec schema ([`crate::registry::typed::level_stem::encode`]: validation + the canonical NBT the
//! codec writes) and once into the typed stem with its built chunk generator and typed dimension type holder.

use crate::registry::codec::{Lookup, RegistryDecodeContext, RegistryLookup};
use crate::registry::typed::dimension_type::DimensionType;
use crate::registry::typed::level_stem;
use crate::registry::{Registry, RegistryAccess, RegistryEntry, RegistrationInfo, keys};
use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::SpecialWorldProperty;
use mc_worldgen::biome::BiomeRegistry;
use mc_worldgen::data::{LoadError, WorldgenRegistries};
use mc_worldgen::densityfunction::codec::LoadResult;
use mc_worldgen::flat::{FlatLevelGeneratorSettings, FlatLevelSource};
use mc_worldgen::world_preset::{self, DimensionTypes, Holder, LevelGenerator, Lifecycle, WorldPreset};
use serde_json::{Map, Number, Value};
use std::sync::Arc;

/// `LevelStem` with its dimension type resolved: the entry type of the baked `dimension` registry.
pub type LevelStem = world_preset::LevelStem<DimensionType>;

/// `WorldDimensions` with typed dimension types.
pub type TypedWorldDimensions = world_preset::WorldDimensions<DimensionType>;

pub fn overworld_key() -> Identifier {
    world_preset::overworld_key()
}

pub use world_preset::{end_key, keys_in_order, nether_key};

/// `RegistryAccess` as the exact `RegistryOps` lookup of a decode.
struct AccessLookup<'a>(&'a RegistryAccess);

impl RegistryLookup for AccessLookup<'_> {
    fn contains(&self, registry: &str, id: &Identifier) -> Lookup {
        match self.0.registry(registry) {
            Some(r) if r.contains(id) => Lookup::Yes,
            Some(_) => Lookup::No,
            None => Lookup::NoRegistry,
        }
    }

    fn tag_exists(&self, registry: &str, tag: &Identifier) -> Lookup {
        match self.0.registry(registry) {
            Some(r) if r.tag(tag).is_some() => Lookup::Yes,
            Some(_) => Lookup::No,
            None => Lookup::NoRegistry,
        }
    }
}

/// `RegistryOps.create(JsonOps, datapackWorldRegistries)` for level stems.
pub struct DimensionDecoder<'a> {
    access: &'a RegistryAccess,
    pub worldgen: Arc<WorldgenRegistries>,
    pub biomes: Arc<BiomeRegistry>,
}

impl<'a> DimensionDecoder<'a> {
    pub fn new(access: &'a RegistryAccess) -> Result<DimensionDecoder<'a>, String> {
        let worldgen = Arc::new(access.worldgen_registries());
        let biomes = Arc::new(BiomeRegistry::load(&worldgen).map_err(|e| format!("Failed to load the biome registry: {e}"))?);
        Ok(DimensionDecoder { access, worldgen, biomes })
    }

    fn with_ctx<R>(&self, f: impl FnOnce(&RegistryDecodeContext) -> R) -> R {
        let lookup = AccessLookup(self.access);
        f(&RegistryDecodeContext::new(&lookup))
    }

    /// `new FlatLevelSource(settings)` for flat settings decoded from `settings_json` (the settings may have failed only
    /// `validateHeight`), and the canonical NBT of the `minecraft:flat` generator holding them.
    pub fn flat_generator(&self, settings: FlatLevelGeneratorSettings, settings_json: &Value) -> Result<(LevelGenerator, Tag), String> {
        let tag = self.with_ctx(|ctx| level_stem::flat_generator_tag(settings_json, ctx, false))?;
        Ok((LevelGenerator::Flat(Arc::new(FlatLevelSource::new(settings, self.biomes.clone()))), tag))
    }

    /// `LevelStem.CODEC` parse + `encodeStart(NbtOps)`: the typed stem and its canonical NBT.
    pub fn stem(&self, json: &Value) -> Result<(Arc<LevelStem>, Tag), String> {
        let tag = self.with_ctx(|ctx| level_stem::encode(json, ctx))?;
        let stem = LevelStem::from_json(json, &self.worldgen, &self.biomes, self).map_err(|e| e.to_string())?;
        Ok((Arc::new(stem), tag))
    }
}

impl DimensionTypes for DimensionDecoder<'_> {
    type Value = DimensionType;

    fn reference(&self, id: &Identifier) -> LoadResult<Arc<DimensionType>> {
        self.access
            .dimension_type(id)
            .cloned()
            .map(Arc::new)
            .ok_or_else(|| LoadError::new(format!("Failed to get element {id} from registry minecraft:dimension_type")))
    }

    fn direct(&self, json: &Value) -> LoadResult<Arc<DimensionType>> {
        self.with_ctx(|ctx| DimensionType::from_json(json, ctx)).map(Arc::new).map_err(LoadError::new)
    }
}

/// `WorldDimensions`: the typed stems plus the canonical NBT each one encodes to (`world_gen_settings.dat` stores it).
#[derive(Clone)]
pub struct WorldDimensions {
    dimensions: TypedWorldDimensions,
    stored: Vec<(Identifier, Tag)>,
}

/// `WorldDimensions.Complete`: the final `dimension` registry and what is derived from it.
pub struct Complete {
    pub dimensions: RegistryAccess,
    pub special_world_property: SpecialWorldProperty,
    pub lifecycle: Lifecycle,
}

impl WorldDimensions {
    /// `WorldPreset.createWorldDimensions`: the preset's `dimensions` object in `dimensionsInOrder` order.
    pub fn from_preset_json(decoder: &DimensionDecoder, preset: &Value) -> Result<WorldDimensions, String> {
        let parsed = WorldPreset::from_json(preset, &decoder.worldgen, &decoder.biomes, decoder).map_err(|e| e.to_string())?;
        let dimensions = parsed.create_world_dimensions();
        let stems = preset.get("dimensions").ok_or("No key dimensions")?;
        let stored = decoder.stored_tags(&dimensions, |key| stems.get(key.to_string()))?;
        Ok(WorldDimensions { dimensions, stored })
    }

    /// `new WorldDimensions(registry)`: the stems of a `dimension` registry.
    pub fn from_registry(decoder: &DimensionDecoder, registry: &Registry) -> Result<WorldDimensions, String> {
        let mut stems = Vec::new();
        let mut stored = Vec::new();
        for entry in registry.entries() {
            let (stem, tag) = decoder.stem(&entry.json).map_err(|e| format!("Failed to parse dimension {}: {e}", entry.id))?;
            stems.push((entry.id.clone(), stem));
            stored.push((entry.id.clone(), tag));
        }
        Ok(WorldDimensions { dimensions: TypedWorldDimensions::new(stems).map_err(|e| e.to_string())?, stored })
    }

    /// `WorldGenSettings.dimensions` (the stored compound of stems).
    pub fn from_stored(decoder: &DimensionDecoder, dimensions: &CompoundTag) -> Result<WorldDimensions, String> {
        let mut stems = Map::new();
        for key in dimensions.keys() {
            stems.insert(key.to_string(), nbt_to_json(dimensions.get(key).expect("key of the compound")));
        }
        let stems = Value::Object(stems);
        let typed = TypedWorldDimensions::from_json(&serde_json::json!({ "dimensions": &stems }), &decoder.worldgen, &decoder.biomes, decoder)
            .map_err(|e| e.to_string())?;
        // keep the stored order: the typed object is in the (sorted) order of the JSON map
        let mut ordered = Vec::new();
        for key in dimensions.keys() {
            let id = Identifier::read(key)?;
            if let Some(stem) = typed.get(&id) {
                ordered.push((id, stem.clone()));
            }
        }
        let typed = TypedWorldDimensions::new(ordered).map_err(|e| e.to_string())?;
        let stored = decoder.stored_tags(&typed, |key| stems.get(key.to_string()))?;
        Ok(WorldDimensions { dimensions: typed, stored })
    }

    pub fn typed(&self) -> &TypedWorldDimensions {
        &self.dimensions
    }

    /// `WorldGenSettings.dimensions` as stored in `world_gen_settings.dat`.
    pub fn to_stored(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        for (key, stem) in &self.stored {
            tag.put(key.to_string(), stem.clone());
        }
        tag
    }

    /// `specialWorldProperty` of the overworld generator.
    pub fn special_world_property(&self) -> SpecialWorldProperty {
        special_property(self.dimensions.special_world_property())
    }

    /// `replaceOverworldGenerator(registries, generator)`: the overworld stem keeps its dimension type and gets `generator`
    /// (`generator_tag` is its canonical NBT, as stored in `LevelStem.generator`).
    pub fn replace_overworld_generator(&mut self, decoder: &DimensionDecoder, generator: LevelGenerator, generator_tag: Tag) -> Result<(), String> {
        self.with_generator(decoder, generator, generator_tag)
    }

    fn with_generator(&mut self, decoder: &DimensionDecoder, generator: LevelGenerator, generator_tag: Tag) -> Result<(), String> {
        let overworld = overworld_key();
        let dimensions = self
            .dimensions
            .replace_overworld_generator(generator, || decoder.reference(&overworld).map(|dimension_type| Holder::Reference(overworld.clone(), dimension_type)))
            .map_err(|e| e.to_string())?;
        // the stored stem: the old overworld compound with its generator replaced (a missing overworld cannot happen)
        let entry = self.stored.iter_mut().find(|(k, _)| *k == overworld_key()).ok_or("Overworld settings missing")?;
        if let Tag::Compound(stem) = &mut entry.1 {
            stem.put("generator", generator_tag);
        }
        self.dimensions = dimensions;
        Ok(())
    }

    /// `bake(baseDimensions)`: the `dimension` registry holds, for every known key, the data pack's stem if there is one,
    /// else this world's; entries are registered as stable only when vanilla-like (and without a known pack).
    pub fn bake(&self, decoder: &DimensionDecoder, base_dimensions: Option<&Registry>) -> Result<Complete, String> {
        let mut base = Vec::new();
        let mut base_json = Vec::new();
        for entry in base_dimensions.map(Registry::entries).unwrap_or_default() {
            let (stem, _) = decoder.stem(&entry.json).map_err(|e| format!("Failed to parse dimension {}: {e}", entry.id))?;
            base.push((entry.id.clone(), stem));
            base_json.push((entry.id.clone(), entry.json.clone()));
        }
        let complete = self.dimensions.bake(&base);
        let mut entries = Vec::new();
        for baked in complete.dimensions {
            let json = match base_json.iter().find(|(k, _)| *k == baked.key) {
                Some((_, json)) => json.clone(),
                None => {
                    let (_, tag) = self.stored.iter().find(|(k, _)| *k == baked.key).expect("baked from this world's dimensions");
                    Arc::new(nbt_to_json(tag))
                }
            };
            entries.push(RegistryEntry {
                id: baked.key,
                json,
                value: Some(baked.stem),
                network: None,
                source_pack: "world".to_owned(),
                info: RegistrationInfo { known_pack: None, stable: baked.lifecycle == Lifecycle::Stable },
            });
        }
        Ok(Complete {
            dimensions: RegistryAccess::from_registries([Registry::dynamic(keys::LEVEL_STEM, entries)]),
            special_world_property: special_property(complete.special_world_property),
            lifecycle: complete.lifecycle,
        })
    }
}

impl DimensionDecoder<'_> {
    /// The canonical NBT of every stem of `dimensions`, encoded from its source JSON.
    fn stored_tags<'v>(&self, dimensions: &TypedWorldDimensions, source: impl Fn(&Identifier) -> Option<&'v Value>) -> Result<Vec<(Identifier, Tag)>, String> {
        dimensions
            .dimensions()
            .iter()
            .map(|(key, _)| {
                let json = source(key).ok_or_else(|| format!("No stem for dimension {key}"))?;
                self.with_ctx(|ctx| level_stem::encode(json, ctx)).map(|tag| (key.clone(), tag))
            })
            .collect()
    }
}

fn special_property(property: world_preset::SpecialWorldProperty) -> SpecialWorldProperty {
    match property {
        world_preset::SpecialWorldProperty::None => SpecialWorldProperty::None,
        world_preset::SpecialWorldProperty::Flat => SpecialWorldProperty::Flat,
        world_preset::SpecialWorldProperty::Debug => SpecialWorldProperty::Debug,
    }
}

/// `NbtOps` -> `JsonOps` (`Dynamic.convert`): compounds/lists/strings/numbers. Integral tags become JSON integers,
/// floating tags floats.
pub fn nbt_to_json(tag: &Tag) -> Value {
    match tag {
        Tag::Compound(c) => {
            let mut map = Map::new();
            for key in c.keys() {
                map.insert(key.to_string(), nbt_to_json(c.get(key).expect("key of the compound")));
            }
            Value::Object(map)
        }
        Tag::List(items) => Value::Array(items.iter().map(nbt_to_json).collect()),
        Tag::String(s) => Value::String(s.to_string()),
        Tag::Byte(v) => Value::Number((*v).into()),
        Tag::Short(v) => Value::Number((*v).into()),
        Tag::Int(v) => Value::Number((*v).into()),
        Tag::Long(v) => Value::Number((*v).into()),
        Tag::Float(v) => Number::from_f64(f64::from(*v)).map_or(Value::Null, Value::Number),
        Tag::Double(v) => Number::from_f64(*v).map_or(Value::Null, Value::Number),
        other => Value::String(other.to_string()),
    }
}
