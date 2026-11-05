//! `AxisAlignedLinearPosTest` (`axis_aligned_linear_pos`).

use super::pos_rule_test::{POS_RULE_TEST_TYPES, PosRuleTest};
use crate::world::level::levelgen::codec::*;
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::direction::{Axis, Direction};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct AxisAlignedLinearPosTest {
    min_chance: f32,
    max_chance: f32,
    min_dist: i32,
    max_dist: i32,
    axis: Axis,
}

/// `Direction.Axis.CODEC` (`StringRepresentable.fromEnum`).
fn parse_axis(value: &serde_json::Value) -> LoadResult<Axis> {
    let name = parse_str(value)?;
    Axis::by_name(name).ok_or_else(|| LoadError::new(format!("Unknown element name:{name}")))
}

impl AxisAlignedLinearPosTest {
    /// The constructor's `IllegalArgumentException` for `minDist >= maxDist` is a decode error.
    pub fn new(min_chance: f32, max_chance: f32, min_dist: i32, max_dist: i32, axis: Axis) -> LoadResult<AxisAlignedLinearPosTest> {
        if min_dist >= max_dist {
            return Err(LoadError::new(format!("Invalid range: [{min_dist},{max_dist}]")));
        }
        Ok(AxisAlignedLinearPosTest { min_chance, max_chance, min_dist, max_dist, axis })
    }

    pub fn decode(f: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PosRuleTest>> {
        Ok(Arc::new(AxisAlignedLinearPosTest::new(
            f.optional("min_chance", parse_f32)?.unwrap_or(0.0),
            f.optional("max_chance", parse_f32)?.unwrap_or(0.0),
            f.optional("min_dist", parse_int)?.unwrap_or(0),
            f.optional("max_dist", parse_int)?.unwrap_or(0),
            f.optional("axis", parse_axis)?.unwrap_or(Axis::Y),
        )?))
    }
}

impl PosRuleTest for AxisAlignedLinearPosTest {
    fn test(&self, _in_template_pos: BlockPos, world_pos: BlockPos, world_reference: BlockPos, random: &mut dyn RandomSource) -> bool {
        // `Direction.get(AxisDirection.POSITIVE, axis)`
        let direction = match self.axis {
            Axis::X => Direction::East,
            Axis::Y => Direction::Up,
            Axis::Z => Direction::South,
        };
        // `Math.abs(int * int)` then widened to float
        let xd = (world_pos.x().wrapping_sub(world_reference.x()).wrapping_mul(direction.get_step_x())).wrapping_abs() as f32;
        let yd = (world_pos.y().wrapping_sub(world_reference.y()).wrapping_mul(direction.get_step_y())).wrapping_abs() as f32;
        let zd = (world_pos.z().wrapping_sub(world_reference.z()).wrapping_mul(direction.get_step_z())).wrapping_abs() as f32;
        let dist = (xd + yd + zd) as i32;
        let rnd = random.next_float();
        rnd <= mth::clamped_lerp_f32(mth::inverse_lerp_f32(dist as f32, self.min_dist as f32, self.max_dist as f32), self.min_chance, self.max_chance)
    }
}

#[linkme::distributed_slice(POS_RULE_TEST_TYPES)]
static TYPE: TypeEntry<dyn PosRuleTest> = TypeEntry { name: "axis_aligned_linear_pos", decode: AxisAlignedLinearPosTest::decode };
