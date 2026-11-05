//! `SurfaceRelativeThresholdFilter`: keeps positions whose y lies within `[surface + min, surface + max]`.

use super::heightmap_placement::parse_heightmap_type;
use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementFilter, PlacementModifier, filter_modify};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_int};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct SurfaceRelativeThresholdFilter {
    pub heightmap: HeightmapType,
    pub min_inclusive: i32,
    pub max_inclusive: i32,
}

impl SurfaceRelativeThresholdFilter {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(SurfaceRelativeThresholdFilter {
            heightmap: fields.required("heightmap", parse_heightmap_type)?,
            min_inclusive: fields.optional("min_inclusive", parse_int)?.unwrap_or(i32::MIN),
            max_inclusive: fields.optional("max_inclusive", parse_int)?.unwrap_or(i32::MAX),
        }))
    }
}

impl PlacementFilter for SurfaceRelativeThresholdFilter {
    fn should_place(&self, context: &PlacementContext<'_>, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let surface_y = i64::from(context.get_height(self.heightmap, origin.x(), origin.z()));
        let min_y = surface_y + i64::from(self.min_inclusive);
        let max_y = surface_y + i64::from(self.max_inclusive);
        min_y <= i64::from(origin.y()) && i64::from(origin.y()) <= max_y
    }
}

impl PlacementModifier for SurfaceRelativeThresholdFilter {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        filter_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static SURFACE_RELATIVE_THRESHOLD_FILTER: TypeEntry<dyn PlacementModifier> =
    TypeEntry { name: "surface_relative_threshold_filter", decode: SurfaceRelativeThresholdFilter::decode };
