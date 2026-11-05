//! `net.minecraft.world.level.storage.loot`: loot tables, pools, entries, conditions, functions, number providers,
//! loot parameters/contexts and loading from the `loot_table` / `predicate` / `item_modifier` / `context_*_provider` /
//! `slot_source` registries.

pub mod block_drops;
pub mod codec_ext;
pub mod context;
pub mod entries;
pub mod functions;
pub mod holder;
pub mod loading;
pub mod map_api;
pub mod parameters;
pub mod predicate_api;
pub mod predicates;
pub mod providers;
pub mod range;
pub mod recipe_api;
pub mod server_access;
pub mod table;
pub mod text_api;
pub mod validation;

pub use context::{ContextKey, ContextKeySet, EntityTarget, LootContext, LootContextBuilder, LootParams, LootParamsBuilder};
pub use parameters::LootContextParamSets;
pub use table::{LootPool, LootTable};
