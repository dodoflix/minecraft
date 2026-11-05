//! `structure.pools.alias`: pool alias bindings (`direct`, `random`, `random_group`) and the lookup resolved per structure
//! start.

pub mod pool_alias_binding;
pub mod pool_alias_lookup;

pub use pool_alias_binding::PoolAliasBinding;
pub use pool_alias_lookup::PoolAliasLookup;
