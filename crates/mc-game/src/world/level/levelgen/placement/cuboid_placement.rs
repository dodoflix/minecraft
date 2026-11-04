//! `CuboidPlacement` (vanilla `feature.CuboidPlacement`): the positions of a randomly sized box starting at the origin.

use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_bool};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct CuboidPlacement {
    pub xz_size: IntProvider,
    pub y_size: IntProvider,
    pub include_edges: bool,
    pub include_interior: bool,
}

impl CuboidPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(CuboidPlacement {
            xz_size: fields.required("xz_size", |v| IntProvider::from_json_range(v, 1, 16))?,
            y_size: fields.required("y_size", |v| IntProvider::from_json_range(v, 1, 16))?,
            include_edges: fields.optional("include_edges", parse_bool)?.unwrap_or(true),
            include_interior: fields.optional("include_interior", parse_bool)?.unwrap_or(true),
        }))
    }
}

impl PlacementModifier for CuboidPlacement {
    fn modify(&self, _context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        let height = self.y_size.sample(random);
        let width = self.xz_size.sample(random);
        let length = self.xz_size.sample(random);
        let edges = self.include_edges;
        for x in 0..=width {
            for y in 0..=height {
                for z in 0..=length {
                    if (edges || (x != 0 && x != width || y != 0 && y != height))
                        && (edges || (z != 0 && z != length || y != 0 && y != height))
                        && (edges || (x != 0 && x != width || z != 0 && z != length))
                        && (self.include_interior || x == 0 || x == width || y == 0 || y == height || z == 0 || z == length)
                    {
                        output.push(BlockPos::new(
                            x.wrapping_add(origin.x()),
                            y.wrapping_add(origin.y()),
                            z.wrapping_add(origin.z()),
                        ));
                    }
                }
            }
        }
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static CUBOID: TypeEntry<dyn PlacementModifier> = TypeEntry { name: "cuboid", decode: CuboidPlacement::decode };
