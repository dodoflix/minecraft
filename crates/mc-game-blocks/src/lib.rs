// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-blocks`: re-exports the lower crate's module tree and adds this crate's modules.

#[macro_use]
extern crate parent;
pub use parent::*;
pub mod world;
