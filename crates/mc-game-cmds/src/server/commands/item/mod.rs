// bridge: remove after branch migration (docs/crate-split.md#removal)
//! `mc-game-cmds`: re-exports the lower crate's module tree and adds this crate's modules.

pub use parent::server::commands::item::*;

pub mod block_item_accessor;

pub mod entity_item_accessor;

pub mod item_accessor;

pub use item_accessor::{ItemAccessor, ItemTarget, SlotSnapshot};

pub mod item_provider;
