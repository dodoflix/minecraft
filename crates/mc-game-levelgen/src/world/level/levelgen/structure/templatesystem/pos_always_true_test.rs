//! `PosAlwaysTrueTest` (`always_true`).

use super::pos_rule_test::{POS_RULE_TEST_TYPES, PosRuleTest};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct PosAlwaysTrueTest;

impl PosRuleTest for PosAlwaysTrueTest {
    fn test(&self, _in_template_pos: BlockPos, _world_pos: BlockPos, _world_reference: BlockPos, _random: &mut dyn RandomSource) -> bool {
        true
    }
}

#[linkme::distributed_slice(POS_RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn PosRuleTest> = TypeEntry { name: "always_true", decode: |_, _| Ok(Arc::new(PosAlwaysTrueTest)) };
