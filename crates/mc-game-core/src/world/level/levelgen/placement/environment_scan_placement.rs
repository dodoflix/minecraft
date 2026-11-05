//! `EnvironmentScanPlacement`: scans up/down from the origin for the first position matching `target_condition`,
//! as long as `allowed_search_condition` holds.

use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier};
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_direction, parse_int_range};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct EnvironmentScanPlacement {
    pub direction_of_search: Direction,
    pub target_condition: Arc<dyn BlockPredicate>,
    /// Default `BlockPredicate.alwaysTrue()`.
    pub allowed_search_condition: Option<Arc<dyn BlockPredicate>>,
    pub max_steps: i32,
}

/// `Direction.VERTICAL_CODEC`.
fn parse_vertical_direction(value: &serde_json::Value) -> LoadResult<Direction> {
    let direction = parse_direction(value)?;
    if matches!(direction, Direction::Up | Direction::Down) { Ok(direction) } else { Err(LoadError::new("Expected a vertical direction")) }
}

impl EnvironmentScanPlacement {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(EnvironmentScanPlacement {
            direction_of_search: fields.required("direction_of_search", parse_vertical_direction)?,
            target_condition: fields.required("target_condition", |v| blockpredicates::decode(v, ctx))?,
            allowed_search_condition: fields.optional("allowed_search_condition", |v| blockpredicates::decode(v, ctx))?,
            max_steps: fields.required("max_steps", |v| parse_int_range(v, 1, 32))?,
        }))
    }

    fn allowed(&self, context: &PlacementContext<'_>, pos: BlockPos) -> bool {
        self.allowed_search_condition.as_ref().is_none_or(|p| p.test(context.level, pos))
    }
}

impl PlacementModifier for EnvironmentScanPlacement {
    fn modify(&self, context: &PlacementContext<'_>, _random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        let mut pos = origin;
        let level = context.level;
        if self.allowed(context, pos) {
            for _ in 0..self.max_steps {
                if self.target_condition.test(level, pos) {
                    output.push(pos);
                    return;
                }
                pos = pos.relative(self.direction_of_search);
                if level.is_outside_build_height(pos.y()) {
                    return;
                }
                if !self.allowed(context, pos) {
                    break;
                }
            }
            if self.target_condition.test(level, pos) {
                output.push(pos);
            }
        }
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static ENVIRONMENT_SCAN: TypeEntry<dyn PlacementModifier> =
    TypeEntry { name: "environment_scan", decode: EnvironmentScanPlacement::decode };
