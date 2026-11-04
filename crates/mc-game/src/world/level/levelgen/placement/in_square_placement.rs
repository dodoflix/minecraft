//! `InSquarePlacement`: a random column of the origin's chunk-sized square.

use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct InSquarePlacement;

impl InSquarePlacement {
    /// `MapCodec.unit(() -> INSTANCE)`.
    fn decode(_fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(InSquarePlacement))
    }
}

impl PlacementModifier for InSquarePlacement {
    fn modify(&self, _context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        let x = random.next_int_bounded(16).wrapping_add(origin.x());
        let z = random.next_int_bounded(16).wrapping_add(origin.z());
        output.push(BlockPos::new(x, origin.y(), z));
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static IN_SQUARE: TypeEntry<dyn PlacementModifier> = TypeEntry { name: "in_square", decode: InSquarePlacement::decode };
