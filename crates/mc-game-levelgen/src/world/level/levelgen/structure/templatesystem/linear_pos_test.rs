//! `LinearPosTest` (`linear_pos`).

use super::pos_rule_test::{POS_RULE_TEST_TYPES, PosRuleTest};
use mc_game_core::world::level::levelgen::codec::*;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct LinearPosTest {
    min_chance: f32,
    max_chance: f32,
    min_dist: i32,
    max_dist: i32,
}

impl LinearPosTest {
    /// The constructor's `IllegalArgumentException` for `minDist >= maxDist` is a decode error.
    pub fn new(min_chance: f32, max_chance: f32, min_dist: i32, max_dist: i32) -> LoadResult<LinearPosTest> {
        if min_dist >= max_dist {
            return Err(LoadError::new(format!("Invalid range: [{min_dist},{max_dist}]")));
        }
        Ok(LinearPosTest { min_chance, max_chance, min_dist, max_dist })
    }

    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PosRuleTest>> {
        Ok(Arc::new(LinearPosTest::new(
            f.optional("min_chance", parse_f32)?.unwrap_or(0.0),
            f.optional("max_chance", parse_f32)?.unwrap_or(0.0),
            f.optional("min_dist", parse_int)?.unwrap_or(0),
            f.optional("max_dist", parse_int)?.unwrap_or(0),
        )?))
    }
}

impl PosRuleTest for LinearPosTest {
    fn test(&self, _in_template_pos: BlockPos, world_pos: BlockPos, world_reference: BlockPos, random: &mut dyn RandomSource) -> bool {
        let dist = world_pos.dist_manhattan(&world_reference);
        let rnd = random.next_float();
        rnd <= mth::clamped_lerp_f32(mth::inverse_lerp_f32(dist as f32, self.min_dist as f32, self.max_dist as f32), self.min_chance, self.max_chance)
    }
}

#[linkme::distributed_slice(POS_RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn PosRuleTest> = TypeEntry { name: "linear_pos", decode: LinearPosTest::decode };
