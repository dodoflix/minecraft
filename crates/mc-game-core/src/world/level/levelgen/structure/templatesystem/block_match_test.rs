//! `BlockMatchTest` (`block_match`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct BlockMatchTest {
    pub block: mc_data::block::Block,
}

impl BlockMatchTest {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(BlockMatchTest { block: f.required("block", parse_block)? }))
    }
}

impl RuleTest for BlockMatchTest {
    fn test(&self, state: BlockState, _pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
        state.block() == self.block
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry { name: "block_match", decode: BlockMatchTest::decode };
