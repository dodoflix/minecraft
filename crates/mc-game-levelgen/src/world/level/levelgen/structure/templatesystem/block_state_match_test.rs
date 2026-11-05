//! `BlockStateMatchTest` (`blockstate_match`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct BlockStateMatchTest {
    pub block_state: BlockState,
}

impl BlockStateMatchTest {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(BlockStateMatchTest { block_state: f.required("block_state", parse_block_state)? }))
    }
}

impl RuleTest for BlockStateMatchTest {
    fn test(&self, state: BlockState, _pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
        state == self.block_state
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry { name: "blockstate_match", decode: BlockStateMatchTest::decode };
