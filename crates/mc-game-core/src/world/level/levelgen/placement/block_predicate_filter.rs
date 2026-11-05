//! `BlockPredicateFilter`: keeps the position when the predicate holds there.

use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementFilter, PlacementModifier, filter_modify};
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct BlockPredicateFilter {
    pub predicate: Arc<dyn BlockPredicate>,
}

impl BlockPredicateFilter {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(BlockPredicateFilter { predicate: fields.required("predicate", |v| blockpredicates::decode(v, ctx))? }))
    }
}

impl PlacementFilter for BlockPredicateFilter {
    fn should_place(&self, context: &PlacementContext<'_>, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        self.predicate.test(context.level, origin)
    }
}

impl PlacementModifier for BlockPredicateFilter {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        filter_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static BLOCK_PREDICATE_FILTER: TypeEntry<dyn PlacementModifier> =
    TypeEntry { name: "block_predicate_filter", decode: BlockPredicateFilter::decode };
