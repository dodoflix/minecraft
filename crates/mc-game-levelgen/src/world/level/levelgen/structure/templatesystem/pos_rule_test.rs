//! `PosRuleTest`: a predicate over the template-relative / world positions of a block (dispatched on `predicate_type`).

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{LoadResult, TypeEntry, decode_dispatch_on};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use serde_json::Value;
use std::sync::Arc;

pub trait PosRuleTest: Send + Sync {
    /// `test(inTemplatePos, worldPos, worldReference, random)`.
    fn test(
        &self,
        in_template_pos: BlockPos,
        world_pos: BlockPos,
        world_reference: BlockPos,
        random: &mut dyn RandomSource,
    ) -> bool;
}

#[linkme::distributed_slice]
pub static POS_RULE_TEST_TYPES: [TypeEntry<dyn PosRuleTest>];

/// `PosRuleTest.CODEC`.
pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PosRuleTest>> {
    decode_dispatch_on("predicate_type", value, ctx, &POS_RULE_TEST_TYPES)
}
