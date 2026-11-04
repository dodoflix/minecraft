//! Worldgen: noise synthesis, density functions, noise router/settings and `RandomState`, plus the
//! state-only chunk generation pipeline (biome sources, noise chunk, aquifer, material rules/surface,
//! carvers, flat/debug generators).
// `DensityFunction` memoizes `range()`/`domain_axes()` in OnceLocks; neither participates in Eq/Hash.
#![allow(clippy::mutable_key_type)]

pub mod aquifer;
pub mod beardifier;
pub mod biome;
pub mod blending;
pub mod carver;
pub mod data;
pub mod debug_level_source;
pub mod densityfunction;
pub mod flat;
pub mod generator;
pub mod material;
pub mod noise_chunk;
pub mod noise_router;
pub mod noise_settings;
pub mod noises;
pub mod provider;
pub mod random_state;
pub mod synth;
pub mod util;
pub mod vertical_anchor;
pub mod world_generation_context;
pub mod world_preset;
