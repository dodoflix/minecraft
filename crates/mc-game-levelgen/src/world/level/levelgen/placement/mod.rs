// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-levelgen`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::levelgen::placement::*;

pub mod biome_filter;

pub mod block_predicate_filter;

pub mod cave_surface;

pub mod count_on_every_layer_placement;

pub mod count_placement;

pub mod environment_scan_placement;

pub mod fixed_placement;

pub mod height_range_placement;

pub mod heightmap_placement;

pub mod in_square_placement;

pub mod noise_based_count_placement;

pub mod noise_threshold_count_placement;

pub mod offset_placement;

pub mod random_chance_placement;

pub mod randomly_selected_placement;

pub mod rarity_filter;

pub mod repeating_placement;

pub mod surface_relative_threshold_filter;

pub mod surface_water_depth_filter;
