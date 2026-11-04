//! `AlwaysTrueTest` (`always_true`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct AlwaysTrueTest;

impl AlwaysTrueTest {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(AlwaysTrueTest))
    }
}

impl RuleTest for AlwaysTrueTest {
    fn test(&self, _state: BlockState, _pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
        true
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry { name: "always_true", decode: AlwaysTrueTest::decode };
