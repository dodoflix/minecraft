//! `net.minecraft.world.item`: `ItemStack` (from `mc_protocol::components`), interaction types, use contexts, the
//! `ItemBehaviour` trait + registry.
mod behaviour;
mod context;
mod interaction;
mod item_utils;
mod stack_ext;
pub mod tool;

pub use crate::world::inventory::ClickAction;
pub use behaviour::*;
pub use context::*;
pub use interaction::*;
pub use item_utils::*;
pub use mc_protocol::components::{HashedStack, ItemStack};
pub use stack_ext::*;
