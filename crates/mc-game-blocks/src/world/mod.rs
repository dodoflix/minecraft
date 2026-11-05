// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-blocks`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::*;
pub mod block_entity;
pub mod level;
