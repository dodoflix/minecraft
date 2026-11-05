// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-monsters`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::world::block_entity::*;

mod creaking_heart_block_entity;

pub use creaking_heart_block_entity::CreakingHeartBlockEntity;
