//! `SurfaceWaterDepthFilter`: keeps positions whose column has at most `max_water_depth` of water on top.

use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementFilter, PlacementModifier, filter_modify};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_int};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct SurfaceWaterDepthFilter {
    pub max_water_depth: i32,
}

impl SurfaceWaterDepthFilter {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(SurfaceWaterDepthFilter { max_water_depth: fields.required("max_water_depth", parse_int)? }))
    }
}

impl PlacementFilter for SurfaceWaterDepthFilter {
    fn should_place(&self, context: &PlacementContext<'_>, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let y_ocean_floor = context.get_height(HeightmapType::OceanFloor, origin.x(), origin.z());
        let y_surface_floor = context.get_height(HeightmapType::WorldSurface, origin.x(), origin.z());
        y_surface_floor.wrapping_sub(y_ocean_floor) <= self.max_water_depth
    }
}

impl PlacementModifier for SurfaceWaterDepthFilter {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        filter_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static SURFACE_WATER_DEPTH_FILTER: TypeEntry<dyn PlacementModifier> =
    TypeEntry { name: "surface_water_depth_filter", decode: SurfaceWaterDepthFilter::decode };
