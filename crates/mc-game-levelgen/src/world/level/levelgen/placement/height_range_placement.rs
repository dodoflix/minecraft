//! `HeightRangePlacement`: the origin at a sampled height.

use mc_game_core::world::level::levelgen::placement::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::HeightProvider;
use std::sync::Arc;

pub struct HeightRangePlacement {
    pub height: HeightProvider,
}

impl HeightRangePlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(HeightRangePlacement { height: fields.required("height", HeightProvider::from_json)? }))
    }
}

impl PlacementModifier for HeightRangePlacement {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        output.push(origin.at_y(self.height.sample(random, &context.world_generation_context)));
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static HEIGHT_RANGE: TypeEntry<dyn PlacementModifier> = TypeEntry { name: "height_range", decode: HeightRangePlacement::decode };
