//! The `requirements` of conditional effects: a `Holder<LootItemCondition>` (`LootItemCondition.CODEC`).
//!
//! INTERIM: the loot system (EcLoot, `world::level::storage::loot::predicates`) is not merged yet. This module is the single
//! place that names its types; the swap is `Requirement` = EcLoot's holder and `codec`/`test` forwarding to it.

use mc_dfu::{Codec, Dynamic, PASSTHROUGH};

/// A condition holder as decoded (interim: the raw `encodeStart` value).
#[derive(Clone)]
pub struct Requirement {
    pub raw: std::sync::Arc<Dynamic>,
}

/// `LootItemCondition.CODEC`.
pub fn requirement_codec() -> Codec<Requirement> {
    PASSTHROUGH.xmap(|d| Requirement { raw: std::sync::Arc::new(d) }, |r: &Requirement| (*r.raw).clone())
}

use super::actors::Actors;
use super::evaluate::ContextKind;
use crate::server::level::ServerLevel;

/// `requirements.value().test(context)` with the context of `kind`.
pub fn requirement_test(_level: &mut ServerLevel, _actors: &Actors<'_>, _requirement: &Requirement, _enchantment_level: i32, _kind: &ContextKind<'_>) -> bool {
    // INTERIM (see module docs): the loot condition evaluation arrives with EcLoot's `LootContext`; until then conditional
    // effects stay inactive (what the pipelines did before this slice existed).
    false
}
