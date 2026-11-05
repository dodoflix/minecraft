//! Data components (`net.minecraft.core.component`, `world.item.component`, ...): typed component values with their
//! network / NBT / JSON / hash codecs, `DataComponentMap`, `DataComponentPatch`, `PatchedDataComponentMap`, `ItemStack`.
//!
//! Layout: [`component`] is the type-erasure framework (`Value`, `ComponentKey<T>`, `ComponentDef`, the registry of
//! defs); [`values`] holds one module per family of component value types, each exporting its typed keys (like
//! `DataComponents.MAX_DAMAGE`) and a `defs()` function that [`registry`] collects.

pub mod common;
pub mod component;
pub mod hashed;
pub mod holder;
pub mod item_stack;
pub mod map;
pub mod registry;
pub mod text_hooks;
pub mod values;

pub use common::{component_codec, custom_data_compound_tag_codec, uuid_codec, DyeColor, Rarity, RegistrySet, SoundEvent};
pub use component::{ComponentDef, ComponentKey, ComponentValue, Value};
pub use hashed::{HashedPatchMap, HashedStack};
pub use holder::{Holder, HolderSet, NoDirect};
pub use item_stack::{ItemStack, ItemStackTemplate};
pub use map::{DataComponentExactPredicate, DataComponentMap, DataComponentMapBuilder, DataComponentPatch, PatchedDataComponentMap, TypedDataComponent};
pub use text_hooks::install_text_hooks;
pub use values::*;
