// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-levelgen`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::levelgen::*;
pub mod blockpredicates;
pub mod feature;
pub mod placement;
pub mod stateproviders;
pub mod structure;

pub mod column;

pub mod geode_block_settings;

pub mod geode_crack_settings;

pub mod geode_layer_settings;

pub mod phantom_spawner;
