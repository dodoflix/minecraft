// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-levelgen`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::level::*;
pub mod levelgen;
