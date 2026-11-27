//! `net.minecraft.world.level.biome`: biomes, climate parameters, biome sources and the biome manager.
//!
//! Biomes are identified by [`BiomeId`](mc_world::palette::BiomeId), their index in the [`BiomeRegistry`]
//! (the dedicated server's registration order: biome identifiers sorted bytewise).

pub mod climate;
mod manager;
mod multi_noise;
pub mod overworld_builder;
mod registry;
mod source;
mod sources;
mod spawn;

pub use manager::{BiomeManager, BiomeResolver};
pub use multi_noise::{
    MultiNoiseBiomeSource, MultiNoiseBiomeSourceParameterList, PARAMETER_LIST_REGISTRY, Preset,
};
pub use registry::{
    BIOME_ATTRIBUTE_VALIDATORS, BIOME_INFO_NOISE, BIOME_REGISTRY, Biome, BiomeGenerationSettings,
    BiomeRegistry, CarverRef, ClimateSettings, FROZEN_TEMPERATURE_NOISE,
    INLINE_PLACED_FEATURE_DECODERS, InlineCarver, InlineFeature, InlinePlacedFeatureDecoder,
    PlacedFeatureRef, Precipitation, TemperatureModifier,
};
pub use source::{
    BIOME_SOURCE_TYPES, BiomeSearchResult, BiomeSource, biome_source_count, biome_source_from_json,
    distinct_biomes,
};
pub use sources::{CheckerboardColumnBiomeSource, FixedBiomeSource, TheEndBiomeSource};
pub use spawn::{NoiseSpawnFinder, SpawnTargetPoint};
