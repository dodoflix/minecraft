//! `RandomBlockStateMatchTest` (`random_blockstate_match`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct RandomBlockStateMatchTest {
    pub block_state: BlockState,
    pub probability: f32,
}

impl RandomBlockStateMatchTest {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(RandomBlockStateMatchTest {
            block_state: f.required("block_state", parse_block_state)?,
            probability: f.required("probability", parse_f32)?,
        }))
    }
}

impl RuleTest for RandomBlockStateMatchTest {
    fn test(&self, state: BlockState, _pos: BlockPos, random: &mut dyn RandomSource) -> bool {
        state == self.block_state && random.next_float() < self.probability
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry {
    name: "random_blockstate_match",
    decode: RandomBlockStateMatchTest::decode,
};
