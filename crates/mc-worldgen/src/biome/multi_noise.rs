//! `MultiNoiseBiomeSource`, `MultiNoiseBiomeSourceParameterList` and its `Preset`s.

use std::sync::Arc;

use serde_json::Value;

use mc_core::ident::Identifier;
use mc_core::pos::quart_pos;
use mc_world::palette::BiomeId;

use super::climate::{self, ParameterList, ParameterPoint, Sampler, TargetPoint};
use super::manager::BiomeResolver;
use super::overworld_builder::OverworldBiomeBuilder;
use super::registry::BiomeRegistry;
use super::source::{BiomeSource, distinct_biomes};
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::codec::{Fields, LoadResult, parse_identifier};
use crate::densityfunction::{DensityBuffer, DensityVolume};

/// The registry folder of `MultiNoiseBiomeSourceParameterList`s.
pub const PARAMETER_LIST_REGISTRY: &str = "worldgen/multi_noise_biome_source_parameter_list";

/// `MultiNoiseBiomeSourceParameterList.Preset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    Nether,
    Overworld,
}

impl Preset {
    /// `Preset.CODEC`: `minecraft:nether` / `minecraft:overworld`.
    pub fn from_id(id: &Identifier) -> LoadResult<Preset> {
        match (id.namespace(), id.path()) {
            ("minecraft", "nether") => Ok(Preset::Nether),
            ("minecraft", "overworld") => Ok(Preset::Overworld),
            _ => Err(LoadError::new(format!("Unknown preset: {id}"))),
        }
    }

    pub fn id(self) -> Identifier {
        let path = match self {
            Preset::Nether => "nether",
            Preset::Overworld => "overworld",
        };
        Identifier::with_default_namespace(path).expect("valid preset id")
    }

    /// The `SourceProvider` applied to the biome keys (paths in the `minecraft` namespace), in vanilla's entry
    /// order.
    pub fn entries(self) -> Vec<(ParameterPoint, &'static str)> {
        match self {
            Preset::Nether => vec![
                (climate::parameters_point(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), "nether_wastes"),
                (climate::parameters_point(0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0), "soul_sand_valley"),
                (climate::parameters_point(0.4, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), "crimson_forest"),
                (climate::parameters_point(0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.375), "warped_forest"),
                (climate::parameters_point(-0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.175), "basalt_deltas"),
            ],
            Preset::Overworld => {
                let mut entries = Vec::new();
                OverworldBiomeBuilder::new().add_biomes(&mut |point, biome| entries.push((point, biome)));
                entries
            }
        }
    }

    /// `provider.apply(lookup)`: the parameter list of the preset with biome keys mapped by `lookup`.
    pub fn parameter_list<T>(self, mut lookup: impl FnMut(&'static str) -> T) -> ParameterList<T> {
        ParameterList::new(
            self.entries()
                .into_iter()
                .map(|(point, key)| (point, lookup(key)))
                .collect(),
        )
    }

    /// `usedBiomes()`: the distinct biome keys.
    pub fn used_biomes(self) -> Vec<&'static str> {
        let mut seen = std::collections::HashSet::new();
        self.entries()
            .into_iter()
            .map(|(_, key)| key)
            .filter(|key| seen.insert(*key))
            .collect()
    }

    /// `knownPresets()`.
    pub fn known_presets() -> [Preset; 2] {
        [Preset::Nether, Preset::Overworld]
    }
}

/// `MultiNoiseBiomeSourceParameterList`: a preset resolved against the biome registry.
pub struct MultiNoiseBiomeSourceParameterList {
    preset: Preset,
    parameters: ParameterList<BiomeId>,
}

impl MultiNoiseBiomeSourceParameterList {
    /// `new MultiNoiseBiomeSourceParameterList(preset, biomes)`.
    pub fn new(
        preset: Preset,
        biomes: &BiomeRegistry,
    ) -> LoadResult<MultiNoiseBiomeSourceParameterList> {
        let mut entries = Vec::new();
        for (point, key) in preset.entries() {
            let id = biomes
                .id_of_name(key)
                .ok_or_else(|| LoadError::new(format!("Failed to get element minecraft:{key}")))?;
            entries.push((point, id));
        }
        Ok(MultiNoiseBiomeSourceParameterList {
            preset,
            parameters: ParameterList::new(entries),
        })
    }

    /// `DIRECT_CODEC` (`{preset}`).
    pub fn from_json(
        value: &Value,
        biomes: &BiomeRegistry,
    ) -> LoadResult<MultiNoiseBiomeSourceParameterList> {
        let fields = Fields::new(value)?;
        let preset = fields.required("preset", |v| Preset::from_id(&parse_identifier(v)?))?;
        Self::new(preset, biomes)
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    pub fn parameters(&self) -> &ParameterList<BiomeId> {
        &self.parameters
    }
}

enum Parameters {
    Direct(ParameterList<BiomeId>),
    /// `Holder<MultiNoiseBiomeSourceParameterList>`: the registry key (if a reference) and the list.
    Preset(Option<Identifier>, Arc<MultiNoiseBiomeSourceParameterList>),
}

/// `MultiNoiseBiomeSource`.
pub struct MultiNoiseBiomeSource {
    parameters: Parameters,
    possible: Vec<BiomeId>,
}

impl MultiNoiseBiomeSource {
    fn new(parameters: Parameters) -> MultiNoiseBiomeSource {
        let list = match &parameters {
            Parameters::Direct(list) => list,
            Parameters::Preset(_, preset) => preset.parameters(),
        };
        let possible = distinct_biomes(list.values().iter().map(|(_, biome)| *biome));
        MultiNoiseBiomeSource { parameters, possible }
    }

    /// `createFromList(parameters)`.
    pub fn create_from_list(parameters: ParameterList<BiomeId>) -> MultiNoiseBiomeSource {
        Self::new(Parameters::Direct(parameters))
    }

    /// `createFromPreset(preset)` for the registry entry `key`.
    pub fn create_from_preset(
        key: Option<Identifier>,
        preset: Arc<MultiNoiseBiomeSourceParameterList>,
    ) -> MultiNoiseBiomeSource {
        Self::new(Parameters::Preset(key, preset))
    }

    /// `MultiNoiseBiomeSource.CODEC`: either `{biomes: [...]}` or `{preset: <holder>}`.
    pub fn from_json(
        value: &Value,
        registries: &WorldgenRegistries,
        biomes: &BiomeRegistry,
    ) -> LoadResult<MultiNoiseBiomeSource> {
        let fields = Fields::new(value)?;
        if value.get("biomes").is_some() {
            let list = fields.required("biomes", |v| {
                ParameterList::from_json(v, |entry| {
                    Fields::new(entry)?.required("biome", |b| biomes.holder_from_json(b))
                })
            })?;
            return Ok(Self::create_from_list(list));
        }
        let (key, preset) = fields.required("preset", |v| match v {
            Value::String(_) => {
                let id = parse_identifier(v)?;
                let list = registries.memoized::<MultiNoiseBiomeSourceParameterList>(
                    PARAMETER_LIST_REGISTRY,
                    &id,
                    |json, _| MultiNoiseBiomeSourceParameterList::from_json(json, biomes),
                )?;
                Ok((Some(id), list))
            }
            _ => Ok((None, Arc::new(MultiNoiseBiomeSourceParameterList::from_json(v, biomes)?))),
        })?;
        Ok(Self::create_from_preset(key, preset))
    }

    fn parameters(&self) -> &ParameterList<BiomeId> {
        match &self.parameters {
            Parameters::Direct(list) => list,
            Parameters::Preset(_, preset) => preset.parameters(),
        }
    }

    /// `stable(expected)`: whether this source uses the registered preset `expected`.
    pub fn stable(&self, expected: &Identifier) -> bool {
        matches!(&self.parameters, Parameters::Preset(Some(key), _) if key == expected)
    }

    /// `getNoiseBiome(target)`.
    pub fn get_noise_biome(&self, target: &TargetPoint) -> BiomeId {
        *self.parameters().find_value(target)
    }

    /// The parameter list in use.
    pub fn parameter_list(&self) -> &ParameterList<BiomeId> {
        self.parameters()
    }
}

impl BiomeSource for MultiNoiseBiomeSource {
    fn possible_biomes(&self) -> &[BiomeId] {
        &self.possible
    }

    fn create_resolver<'a>(&'a self, sampler: Sampler) -> Box<dyn BiomeResolver + 'a> {
        Box::new(move |quart_x: i32, quart_y: i32, quart_z: i32| {
            self.get_noise_biome(&sampler.sample(quart_x, quart_y, quart_z))
        })
    }

    fn create_resolver_for_chunk<'a>(
        &'a self,
        sampler: Sampler,
        min_quart_x: i32,
        min_quart_y: i32,
        min_quart_z: i32,
        quart_size_x: i32,
        quart_size_y: i32,
        quart_size_z: i32,
    ) -> Box<dyn BiomeResolver + 'a> {
        let volume = DensityVolume::new(
            quart_size_x,
            quart_size_y,
            quart_size_z,
            quart_pos::to_block(min_quart_x),
            quart_pos::to_block(min_quart_y),
            quart_pos::to_block(min_quart_z),
            4,
            4,
            4,
        );
        let mut temperature = DensityBuffer::create_unpooled(volume.size());
        let mut vegetation = DensityBuffer::create_unpooled(volume.size());
        let mut continents = DensityBuffer::create_unpooled(volume.size());
        let mut erosion = DensityBuffer::create_unpooled(volume.size());
        let mut depth = DensityBuffer::create_unpooled(volume.size());
        let mut ridges = DensityBuffer::create_unpooled(volume.size());
        sampler.temperature().sample_volume(&mut temperature, &volume);
        sampler.humidity().sample_volume(&mut vegetation, &volume);
        sampler.continentalness().sample_volume(&mut continents, &volume);
        sampler.erosion().sample_volume(&mut erosion, &volume);
        sampler.depth().sample_volume(&mut depth, &volume);
        sampler.weirdness().sample_volume(&mut ridges, &volume);
        Box::new(move |quart_x: i32, quart_y: i32, quart_z: i32| {
            let index = volume.index_unchecked(
                quart_x.wrapping_sub(min_quart_x),
                quart_y.wrapping_sub(min_quart_y),
                quart_z.wrapping_sub(min_quart_z),
            );
            self.get_noise_biome(&climate::target(
                temperature.get(index),
                vegetation.get(index),
                continents.get(index),
                erosion.get(index),
                depth.get(index),
                ridges.get(index),
            ))
        })
    }
}
