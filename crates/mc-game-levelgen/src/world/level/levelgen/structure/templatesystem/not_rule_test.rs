//! `NotRuleTest` (`not`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use std::sync::Arc;

pub struct NotRuleTest {
    pub rule: Arc<dyn RuleTest>,
}

impl NotRuleTest {
    pub fn decode(f: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(NotRuleTest {
            rule: f.required("rule", |v| super::rule_test::decode(v, ctx))?,
        }))
    }
}

impl RuleTest for NotRuleTest {
    fn test(&self, state: BlockState, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
        !self.rule.test(state, pos, random)
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry {
    name: "not",
    decode: NotRuleTest::decode,
};
