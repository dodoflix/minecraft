//! `RarityFilter`: keeps the position with probability `1 / chance`.

use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementFilter, PlacementModifier, filter_modify};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_positive_int};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct RarityFilter {
    pub chance: i32,
}

impl RarityFilter {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(RarityFilter { chance: fields.required("chance", parse_positive_int)? }))
    }
}

impl PlacementFilter for RarityFilter {
    fn should_place(&self, _context: &PlacementContext<'_>, random: &mut dyn RandomSource, _origin: BlockPos) -> bool {
        random.next_float() < 1.0f32 / self.chance as f32
    }
}

impl PlacementModifier for RarityFilter {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        filter_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static RARITY_FILTER: TypeEntry<dyn PlacementModifier> = TypeEntry { name: "rarity_filter", decode: RarityFilter::decode };
