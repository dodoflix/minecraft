//! `PoolAliasBinding` and its three implementations (`DirectPoolAlias`, `RandomPoolAlias`, `RandomGroupPoolAlias`).
//!
//! `BuiltInRegistries.POOL_ALIAS_BINDING_TYPE` is a closed registry of three entries, so the bindings are one enum
//! dispatched on `type` (`PoolAliasBindings.bootstrap`: `random`, `random_group`, `direct`).

use mc_game_core::world::level::levelgen::codec::*;
use mc_core::ident::Identifier;
use mc_core::random::RandomSource;
use mc_worldgen::provider::WeightedList;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum PoolAliasBinding {
    /// `DirectPoolAlias(alias, target)`.
    Direct { alias: Identifier, target: Identifier },
    /// `RandomPoolAlias(alias, targets)`.
    Random { alias: Identifier, targets: WeightedList<Identifier> },
    /// `RandomGroupPoolAlias(groups)`.
    RandomGroup { groups: WeightedList<Vec<PoolAliasBinding>> },
}

/// `ResourceKey.codec(Registries.TEMPLATE_POOL)`: an identifier string.
fn parse_pool_key(value: &Value) -> LoadResult<Identifier> {
    parse_identifier(value)
}

impl PoolAliasBinding {
    /// `PoolAliasBinding.CODEC`.
    pub fn from_json(value: &Value) -> LoadResult<PoolAliasBinding> {
        let fields = Fields::new(value)?;
        let ty = fields.required("type", parse_identifier)?;
        if ty.namespace() != "minecraft" {
            return Err(LoadError::new(format!("Unknown registry key in dispatch: {ty}")).at("type"));
        }
        match ty.path() {
            "direct" => Ok(PoolAliasBinding::Direct {
                alias: fields.required("alias", parse_pool_key)?,
                target: fields.required("target", parse_pool_key)?,
            }),
            "random" => Ok(PoolAliasBinding::Random {
                alias: fields.required("alias", parse_pool_key)?,
                targets: fields.required("targets", |v| WeightedList::non_empty_from_json(v, parse_pool_key))?,
            }),
            "random_group" => Ok(PoolAliasBinding::RandomGroup {
                groups: fields.required("groups", |v| WeightedList::non_empty_from_json(v, |g| parse_list(g, PoolAliasBinding::from_json)))?,
            }),
            _ => Err(LoadError::new(format!("Unknown registry key in dispatch: {ty}")).at("type")),
        }
    }

    /// `forEachResolved(random, consumer)`.
    pub fn for_each_resolved(&self, random: &mut dyn RandomSource, consumer: &mut dyn FnMut(&Identifier, &Identifier)) {
        match self {
            PoolAliasBinding::Direct { alias, target } => consumer(alias, target),
            PoolAliasBinding::Random { alias, targets } => consumer(alias, targets.get_random_or_throw(random)),
            PoolAliasBinding::RandomGroup { groups } => {
                for binding in groups.get_random_or_throw(random) {
                    binding.for_each_resolved(random, consumer);
                }
            }
        }
    }

    /// `allTargets()`.
    pub fn all_targets(&self) -> Vec<&Identifier> {
        match self {
            PoolAliasBinding::Direct { target, .. } => vec![target],
            PoolAliasBinding::Random { targets, .. } => targets.unwrap().iter().map(|w| &w.value).collect(),
            PoolAliasBinding::RandomGroup { groups } => {
                groups.unwrap().iter().flat_map(|w| w.value.iter()).flat_map(PoolAliasBinding::all_targets).collect()
            }
        }
    }
}
