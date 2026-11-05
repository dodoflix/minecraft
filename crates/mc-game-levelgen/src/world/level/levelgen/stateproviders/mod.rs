// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-levelgen`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::levelgen::stateproviders::*;

pub mod copy_properties_provider;

pub mod dual_noise_provider;

pub mod noise_based_state_provider;

pub mod noise_provider;

pub mod noise_threshold_provider;

pub mod random_block_provider;

pub mod randomized_int_state_provider;

pub mod rotated_block_provider;

pub mod rule_based_state_provider;

pub mod weighted_state_provider;
