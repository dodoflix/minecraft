//! `RuleBlockEntityModifier` (dispatched on `type`).

use crate::world::level::levelgen::codec::{LoadResult, TypeEntry, decode_dispatch};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::random::RandomSource;
use mc_nbt::CompoundTag;
use serde_json::Value;
use std::sync::Arc;

pub trait RuleBlockEntityModifier: Send + Sync {
    /// `apply(random, existingTag)`.
    fn apply(&self, random: &mut dyn RandomSource, existing_tag: Option<&CompoundTag>) -> Option<CompoundTag>;
}

#[linkme::distributed_slice]
pub static RULE_BLOCK_ENTITY_MODIFIER_TYPES: [TypeEntry<dyn RuleBlockEntityModifier>];

/// `RuleBlockEntityModifier.CODEC`.
pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleBlockEntityModifier>> {
    decode_dispatch(value, ctx, &RULE_BLOCK_ENTITY_MODIFIER_TYPES)
}
