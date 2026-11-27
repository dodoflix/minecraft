//! Golden tests (T0/T1): replays of vanilla recordings and hermetic checks; one module per topic.

// links the game crates: they register the checks an inline biome's `attributes` go through and the decoders of the
// features and processors an inline placed feature refers to
use mc_game_core as _;
use mc_game_levelgen as _;

mod beardifier;
mod biome_codec;
mod biome_params;
mod biome_sampling;
mod blending;
mod carvers;
mod common;
mod end_island;
mod flat_golden;
mod loader;
mod material_codec;
mod material_surface;
mod pipeline_golden;
mod providers;
mod random_state;
mod structure_placement;
mod synth_golden;
mod terrain_golden;
mod world_dimensions_golden;
mod world_presets_golden;
