//! `AllOfRuleTest` (`all_of`).

use super::rule_test::{RULE_TEST_TYPES, RuleTest};
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct AllOfRuleTest {
    pub rules: Vec<Arc<dyn RuleTest>>,
}

impl AllOfRuleTest {
    pub fn decode(f: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn RuleTest>> {
        Ok(Arc::new(AllOfRuleTest { rules: f.required("rules", |v| parse_list(v, |v| super::rule_test::decode(v, ctx)))? }))
    }
}

impl RuleTest for AllOfRuleTest {
    fn test(&self, state: BlockState, pos: BlockPos, random: &mut dyn RandomSource) -> bool {
        self.rules.iter().all(|rule| rule.test(state, pos, random))
    }
}

#[linkme::distributed_slice(RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn RuleTest> = TypeEntry { name: "all_of", decode: AllOfRuleTest::decode };
