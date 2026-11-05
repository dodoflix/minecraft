//! `CountPlacement`: the origin repeated `count` times.

use super::repeating_placement::{RepeatingPlacement, repeating_modify};
use mc_game_core::world::level::levelgen::placement::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct CountPlacement {
    pub count: IntProvider,
}

impl CountPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(CountPlacement { count: fields.required("count", |v| IntProvider::from_json_range(v, 0, 4096))? }))
    }
}

impl RepeatingPlacement for CountPlacement {
    fn count(&self, random: &mut dyn RandomSource, _origin: BlockPos) -> i32 {
        self.count.sample(random)
    }
}

impl PlacementModifier for CountPlacement {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        repeating_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static COUNT: TypeEntry<dyn PlacementModifier> = TypeEntry { name: "count", decode: CountPlacement::decode };
