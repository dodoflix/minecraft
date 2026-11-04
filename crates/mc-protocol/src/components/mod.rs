//! Data components (`net.minecraft.core.component`, `world.item.component`, ...): typed component values with their
//! network / NBT / JSON / hash codecs, `DataComponentMap`, `DataComponentPatch`, `PatchedDataComponentMap`, `ItemStack`.
//!
//! Layout: [`component`] is the type-erasure framework (`Value`, `ComponentKey<T>`, `ComponentDef`, the registry of
//! defs); [`values`] holds one module per family of component value types, each exporting its typed keys (like
//! `DataComponents.MAX_DAMAGE`) and a `defs()` function that [`registry`] collects.

pub mod component;
pub mod hashed;
pub mod holder;
pub mod item_stack;
pub mod map;
pub mod registry;
pub mod values;

pub use component::{ComponentDef, ComponentKey, ComponentValue, Value};
pub use hashed::{HashedPatchMap, HashedStack};
pub use holder::{Holder, HolderSet, NoDirect};
pub use item_stack::{ItemStack, ItemStackTemplate};
pub use map::{DataComponentMap, DataComponentPatch, PatchedDataComponentMap, TypedDataComponent};
pub use values::*;
