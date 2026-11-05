//! `net.minecraft.server.commands.item`: the `/item` accessors, plus the slot plumbing they share with `/loot`.
pub mod block_item_accessor;
pub mod entity_item_accessor;
pub mod item_accessor;
pub mod item_provider;
pub mod slots;

pub use item_accessor::{ItemAccessor, ItemTarget, SlotSnapshot};
