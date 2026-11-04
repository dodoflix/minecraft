//! `HeightMatchTest` (`height_match`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct HeightMatchTest {
    pub min_inclusive: i32,
    pub max_inclusive: i32,
}

impl HeightMatchTest {
    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(HeightMatchTest { min_inclusive: f.required("min_inclusive", parse_int)?, max_inclusive: f.required("max_inclusive", parse_int)? }))
    }
}

impl RuleTest for HeightMatchTest {
    fn test(&self, _state: BlockState, pos: BlockPos, _random: &mut dyn RandomSource) -> bool {
        self.min_inclusive <= pos.y() && pos.y() <= self.max_inclusive
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry { name: "height_match", decode: HeightMatchTest::decode };
