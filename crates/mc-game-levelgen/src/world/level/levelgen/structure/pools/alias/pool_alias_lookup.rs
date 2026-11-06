//! `PoolAliasLookup`.

use super::PoolAliasBinding;
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::random::{self, RandomSource};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub enum PoolAliasLookup {
    /// `PoolAliasLookup.EMPTY`: every key maps to itself.
    #[default]
    Empty,
    Mappings(HashMap<Identifier, Identifier>),
}

impl PoolAliasLookup {
    /// `lookup(alias)`.
    pub fn lookup(&self, alias: &Identifier) -> Identifier {
        match self {
            PoolAliasLookup::Empty => alias.clone(),
            PoolAliasLookup::Mappings(mappings) => mappings.get(alias).unwrap_or(alias).clone(),
        }
    }

    /// `PoolAliasLookup.create(poolAliasBindings, pos, seed)`. Panics on a duplicate alias like vanilla's
    /// `ImmutableMap.Builder.build()` (`IllegalArgumentException`).
    pub fn create(
        pool_alias_bindings: &[PoolAliasBinding],
        pos: BlockPos,
        seed: i64,
    ) -> PoolAliasLookup {
        if pool_alias_bindings.is_empty() {
            return PoolAliasLookup::Empty;
        }
        let mut random = random::create(seed).fork_positional().at_pos(pos);
        let mut mappings = HashMap::new();
        for binding in pool_alias_bindings {
            binding.for_each_resolved(
                &mut *random as &mut dyn RandomSource,
                &mut |alias, target| {
                    if mappings.insert(alias.clone(), target.clone()).is_some() {
                        panic!("Multiple entries with same key: {alias}");
                    }
                },
            );
        }
        PoolAliasLookup::Mappings(mappings)
    }
}
