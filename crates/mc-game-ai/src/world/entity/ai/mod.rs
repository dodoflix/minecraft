// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-ai`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::entity::ai::*;
pub mod behavior;
pub mod goal;
