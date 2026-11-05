//! `RandomBlockMatchTest` (`random_block_match`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct RandomBlockMatchTest {
    pub block: mc_data::block::Block,
    pub probability: f32,
}

impl RandomBlockMatchTest {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(RandomBlockMatchTest { block: f.required("block", parse_block)?, probability: f.required("probability", parse_f32)? }))
    }
}

impl RuleTest for RandomBlockMatchTest {
    fn test(&self, state: BlockState, _pos: BlockPos, random: &mut dyn RandomSource) -> bool {
        state.block() == self.block && random.next_float() < self.probability
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry { name: "random_block_match", decode: RandomBlockMatchTest::decode };
