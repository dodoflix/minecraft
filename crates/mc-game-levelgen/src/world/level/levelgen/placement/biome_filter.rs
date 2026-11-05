//! `BiomeFilter`: keeps the position when the biome there lists the feature being placed.

use mc_game_core::world::level::levelgen::placement::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementFilter, PlacementModifier, filter_modify};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use std::sync::Arc;

pub struct BiomeFilter;

impl BiomeFilter {
    /// `MapCodec.unit(() -> INSTANCE)`.
    fn decode(_fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(BiomeFilter))
    }
}

impl PlacementFilter for BiomeFilter {
    fn should_place(&self, context: &PlacementContext<'_>, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let feature = context
            .top_feature
            .expect("Tried to biome check an unregistered feature, or a feature that should not restrict the biome");
        let biome = context.level.get_biome(origin);
        context.generator.get_biome_generation_settings(biome).features().iter().flatten().any(|f| f == feature)
    }
}

impl PlacementModifier for BiomeFilter {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        filter_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static BIOME: TypeEntry<dyn PlacementModifier> = TypeEntry { name: "biome", decode: BiomeFilter::decode };
