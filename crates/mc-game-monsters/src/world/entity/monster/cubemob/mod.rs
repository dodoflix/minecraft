// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-monsters`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::entity::monster::cubemob::*;

pub mod magma_cube;

pub mod slime;

pub mod sulfur_cube;
