//! `FlatLevelGeneratorSettings`: layer list, biome, structure overrides, `features`/`lakes` flags, the JSON codec and
//! `adjustGenerationSettings` (the per-biome generation settings a flat world uses).

use std::borrow::Cow;

use mc_core::ident::Identifier;
use mc_data::block::{Block, BlockState};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;
use serde_json::Value;

use super::layer_info::FlatLayerInfo;
use crate::biome::{BiomeGenerationSettings, BiomeRegistry, PlacedFeatureRef};
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::codec::{Fields, LoadResult, parse_bool, parse_identifier, parse_list};
use crate::noise_settings::DIMENSION_Y_SIZE;

pub const STRUCTURE_SET_REGISTRY: &str = "worldgen/structure_set";
pub const STRUCTURE_SET_TAG_REGISTRY: &str = "tags/worldgen/structure_set";
pub const PLACED_FEATURE_REGISTRY: &str = "worldgen/placed_feature";

/// `GenerationStep.Decoration` ordinals used by the flat generator.
pub mod decoration {
    pub const LAKES: usize = 1;
    pub const UNDERGROUND_STRUCTURES: usize = 3;
    pub const SURFACE_STRUCTURES: usize = 4;
    pub const TOP_LAYER_MODIFICATION: usize = 10;
}

/// `HolderSet<StructureSet>` as the flat settings keep it: a tag reference or a direct list of registry ids.
/// Elements/tags are validated against the data source when decoded, like `RegistryOps` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureSetHolderSet {
    Tag(Identifier),
    Direct(Vec<Identifier>),
}

impl StructureSetHolderSet {
    /// `RegistryCodecs.holderSet(Registries.STRUCTURE_SET)`: `#tag`, a single id, or a list of ids.
    pub fn from_json(value: &Value, registries: &WorldgenRegistries) -> LoadResult<StructureSetHolderSet> {
        let element = |v: &Value| -> LoadResult<Identifier> {
            let id = parse_identifier(v)?;
            if registries.raw(STRUCTURE_SET_REGISTRY, &id).is_none() {
                return Err(LoadError::new(format!("Unknown registry key in minecraft:worldgen/structure_set: {id}")));
            }
            Ok(id)
        };
        if let Some(tag) = value.as_str().and_then(|s| s.strip_prefix('#')) {
            let id = Identifier::read(tag).map_err(LoadError::new)?;
            if registries.raw(STRUCTURE_SET_TAG_REGISTRY, &id).is_none() {
                return Err(LoadError::new(format!("Missing tag: '{id}' in 'minecraft:worldgen/structure_set'")));
            }
            return Ok(StructureSetHolderSet::Tag(id));
        }
        if value.is_array() {
            return parse_list(value, element).map(StructureSetHolderSet::Direct);
        }
        element(value).map(|id| StructureSetHolderSet::Direct(vec![id]))
    }
}

/// `BiomeGenerationSettings.PlainBuilder` without carvers (flat settings never add any).
#[derive(Default)]
struct PlainBuilder {
    features: Vec<Vec<PlacedFeatureRef>>,
}

impl PlainBuilder {
    fn add_feature(&mut self, index: usize, feature: PlacedFeatureRef) {
        while self.features.len() <= index {
            self.features.push(Vec::new());
        }
        self.features[index].push(feature);
    }

    fn build(self) -> BiomeGenerationSettings {
        BiomeGenerationSettings::new(Vec::new(), self.features)
    }
}

#[derive(Clone, Debug)]
pub struct FlatLevelGeneratorSettings {
    structure_overrides: Option<StructureSetHolderSet>,
    layers_info: Vec<FlatLayerInfo>,
    biome: BiomeId,
    /// `layers`: one entry per Y layer; `None` once `adjust_generation_settings` replaced a non-motion-blocking layer
    /// by a fill-layer feature (vanilla nulls the shared list entry).
    layers: Vec<Option<BlockState>>,
    void_gen: bool,
    decoration: bool,
    add_lakes: bool,
    lakes: Vec<Identifier>,
}

impl FlatLevelGeneratorSettings {
    /// The public constructor: no layers yet.
    pub fn new(
        structure_overrides: Option<StructureSetHolderSet>,
        biome: BiomeId,
        lakes: Vec<Identifier>,
    ) -> FlatLevelGeneratorSettings {
        FlatLevelGeneratorSettings {
            structure_overrides,
            layers_info: Vec::new(),
            biome,
            layers: Vec::new(),
            void_gen: false,
            decoration: false,
            add_lakes: false,
            lakes,
        }
    }

    /// `FlatLevelGeneratorSettings.CODEC` (incl. `validateHeight`).
    pub fn from_json(
        value: &Value,
        registries: &WorldgenRegistries,
        biomes: &BiomeRegistry,
    ) -> LoadResult<FlatLevelGeneratorSettings> {
        let fields = Fields::new(value)?;
        // `lenientOptionalFieldOf`: an invalid value is the same as an absent one.
        let structure_overrides = fields
            .optional("structure_overrides", |v| StructureSetHolderSet::from_json(v, registries))
            .unwrap_or(None);
        let layers = fields.required("layers", |v| parse_list(v, FlatLayerInfo::from_json))?;
        let lakes = fields.optional("lakes", parse_bool)?.unwrap_or(false);
        let features = fields.optional("features", parse_bool)?.unwrap_or(false);
        let biome = decode_lenient_biome(value, biomes)?;
        let plains = biomes
            .id_of(&default_biome_id())
            .ok_or_else(|| LoadError::new("Missing element minecraft:plains in minecraft:worldgen/biome"))?;
        let biome = match biome {
            Some(biome) => biome,
            None => {
                log::error!("Unknown biome, defaulting to plains");
                plains
            }
        };
        let lake_list = create_lakes_list(registries)?;

        let mut settings = FlatLevelGeneratorSettings::new(structure_overrides, biome, lake_list);
        if lakes {
            settings.set_add_lakes();
        }
        if features {
            settings.set_decoration();
        }
        settings.layers_info.extend(layers);
        settings.update_layers();
        settings.validate_height()?;
        Ok(settings)
    }

    /// `validateHeight`: the (wrapping, like `IntStream.sum`) layer height sum must fit the dimension.
    fn validate_height(&self) -> LoadResult<()> {
        let total = self.layers_info.iter().fold(0i32, |sum, l| sum.wrapping_add(l.get_height()));
        if total > DIMENSION_Y_SIZE {
            return Err(LoadError::new(format!("Sum of layer heights is > {DIMENSION_Y_SIZE}")));
        }
        Ok(())
    }

    /// `withBiomeAndLayers`: same lakes/flags, new layers, biome and structure overrides.
    pub fn with_biome_and_layers(
        &self,
        layers: &[FlatLayerInfo],
        structure_overrides: Option<StructureSetHolderSet>,
        biome: BiomeId,
    ) -> FlatLevelGeneratorSettings {
        let mut settings = FlatLevelGeneratorSettings::new(structure_overrides, biome, self.lakes.clone());
        for layer in layers {
            settings.layers_info.push(*layer);
            settings.update_layers();
        }
        if self.decoration {
            settings.set_decoration();
        }
        if self.add_lakes {
            settings.set_add_lakes();
        }
        settings
    }

    pub fn set_decoration(&mut self) {
        self.decoration = true;
    }

    pub fn set_add_lakes(&mut self) {
        self.add_lakes = true;
    }

    pub fn has_decoration(&self) -> bool {
        self.decoration
    }

    pub fn has_lakes(&self) -> bool {
        self.add_lakes
    }

    pub fn is_void_gen(&self) -> bool {
        self.void_gen
    }

    pub fn lakes(&self) -> &[Identifier] {
        &self.lakes
    }

    /// `adjustGenerationSettings`. Mutates the layer list exactly like vanilla: every layer that is not
    /// `MOTION_BLOCKING`-opaque becomes `None` here and a fill-layer feature takes over.
    pub fn adjust_generation_settings<'a>(
        &mut self,
        biomes: &'a BiomeRegistry,
        source_biome: BiomeId,
    ) -> Cow<'a, BiomeGenerationSettings> {
        if source_biome != self.biome {
            return Cow::Borrowed(biomes.get(source_biome).generation_settings());
        }
        let biome_settings = biomes.get(self.biome).generation_settings();
        let mut builder = PlainBuilder::default();
        if self.add_lakes {
            for lake in &self.lakes {
                builder.add_feature(decoration::LAKES, PlacedFeatureRef::Named(lake.clone()));
            }
        }

        let source_is_void = biomes.key(source_biome) == &Identifier::with_default_namespace("the_void").expect("valid id");
        let biome_decoration = (!self.void_gen || source_is_void) && self.decoration;
        if biome_decoration {
            for (step_index, step) in biome_settings.features.iter().enumerate() {
                if step_index != decoration::UNDERGROUND_STRUCTURES
                    && step_index != decoration::SURFACE_STRUCTURES
                    && (!self.add_lakes || step_index != decoration::LAKES)
                {
                    for feature in step {
                        builder.add_feature(step_index, feature.clone());
                    }
                }
            }
        }

        for (i, layer) in self.layers.iter_mut().enumerate() {
            if let Some(state) = *layer
                && !HeightmapType::MotionBlocking.is_opaque(state)
            {
                *layer = None;
                builder.add_feature(
                    decoration::TOP_LAYER_MODIFICATION,
                    PlacedFeatureRef::InlineFillLayer { height: i as i32, state },
                );
            }
        }
        Cow::Owned(builder.build())
    }

    pub fn structure_overrides(&self) -> Option<&StructureSetHolderSet> {
        self.structure_overrides.as_ref()
    }

    pub fn get_biome(&self) -> BiomeId {
        self.biome
    }

    pub fn get_layers_info(&self) -> &[FlatLayerInfo] {
        &self.layers_info
    }

    /// `getLayersInfo().add(info)` (callers run [`Self::update_layers`] afterwards, as vanilla's do).
    pub fn add_layer_info(&mut self, info: FlatLayerInfo) {
        self.layers_info.push(info);
    }

    /// `getLayers()`.
    pub fn get_layers(&self) -> &[Option<BlockState>] {
        &self.layers
    }

    /// `updateLayers`: flattens the layer infos and recomputes `voidGen` (all layers air; true for no layers).
    pub fn update_layers(&mut self) {
        self.layers.clear();
        for layer in &self.layers_info {
            for _ in 0..layer.get_height() {
                self.layers.push(Some(layer.get_block_state()));
            }
        }
        self.void_gen = self.layers.iter().all(|s| s.is_some_and(|s| s.block() == Block::AIR));
    }

    /// `getDefault`: stronghold + village structure overrides, plains, bedrock / 2 dirt / grass block.
    pub fn get_default(registries: &WorldgenRegistries, biomes: &BiomeRegistry) -> LoadResult<FlatLevelGeneratorSettings> {
        let mut structure_sets = Vec::new();
        for name in ["strongholds", "villages"] {
            let id = Identifier::with_default_namespace(name).expect("valid id");
            if registries.raw(STRUCTURE_SET_REGISTRY, &id).is_none() {
                return Err(LoadError::new(format!("Missing element {id} in minecraft:{STRUCTURE_SET_REGISTRY}")));
            }
            structure_sets.push(id);
        }
        let mut result = FlatLevelGeneratorSettings::new(
            Some(StructureSetHolderSet::Direct(structure_sets)),
            get_default_biome(biomes)?,
            create_lakes_list(registries)?,
        );
        result.add_layer_info(FlatLayerInfo::new(1, Block::BEDROCK));
        result.add_layer_info(FlatLayerInfo::new(2, Block::DIRT));
        result.add_layer_info(FlatLayerInfo::new(1, Block::GRASS_BLOCK));
        result.update_layers();
        Ok(result)
    }
}

fn default_biome_id() -> Identifier {
    Identifier::with_default_namespace("plains").expect("valid id")
}

/// `getDefaultBiome`.
pub fn get_default_biome(biomes: &BiomeRegistry) -> LoadResult<BiomeId> {
    biomes
        .id_of(&default_biome_id())
        .ok_or_else(|| LoadError::new("Missing element minecraft:plains in minecraft:worldgen/biome"))
}

/// `createLakesList`: underground then surface lava lake.
pub fn create_lakes_list(registries: &WorldgenRegistries) -> LoadResult<Vec<Identifier>> {
    ["lake_lava_underground", "lake_lava_surface"]
        .into_iter()
        .map(|name| {
            let id = Identifier::with_default_namespace(name).expect("valid id");
            if registries.raw(PLACED_FEATURE_REGISTRY, &id).is_none() {
                return Err(LoadError::new(format!("Missing element {id} in minecraft:{PLACED_FEATURE_REGISTRY}")));
            }
            Ok(id)
        })
        .collect()
}

/// `Biome.CODEC.lenientOptionalFieldOf("biome")`: a missing, malformed or unregistered biome is `None`.
/// An inline biome definition (object) is valid in vanilla but cannot be represented by a registry id here.
fn decode_lenient_biome(value: &Value, biomes: &BiomeRegistry) -> LoadResult<Option<BiomeId>> {
    match value.get("biome") {
        Some(Value::Object(_)) => Err(LoadError::new("inline biome definitions are not supported").at("biome")),
        Some(Value::String(s)) => Ok(Identifier::read(s).ok().and_then(|id| biomes.id_of(&id))),
        _ => Ok(None),
    }
}
