//! `RuleTest`: a predicate over a block state (and the world position / random) used by processor rules and ore targets.
//! Each type lives in its own file and registers itself in [`RULE_TEST_TYPES`] (dispatched on `predicate_type`).

use mc_game_core::world::level::LevelReader;
use mc_game_core::world::level::levelgen::codec::{LoadResult, TypeEntry, decode_dispatch_on};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use serde_json::Value;
use std::sync::Arc;

pub trait RuleTest: Send + Sync {
    /// `testAgainstWorldState(level, pos, random)`.
    fn test_against_world_state(&self, level: &dyn LevelReader, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
        self.test(level.get_block_state(pos), pos, random)
    }

    /// `test(state, pos, random)`.
    fn test(&self, state: BlockState, pos: BlockPos, random: &mut dyn RandomSource) -> bool;
}

#[linkme::distributed_slice]
pub static RULE_TEST_TYPES: [TypeEntry<dyn RuleTest>];

/// `RuleTest.CODEC`.
pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
    decode_dispatch_on("predicate_type", value, ctx, &RULE_TEST_TYPES)
}
