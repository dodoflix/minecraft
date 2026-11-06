//! `RandomChancePlacement`: keeps the position with probability `chance`.

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_float_range,
};
use mc_game_core::world::level::levelgen::placement::{
    PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementFilter, PlacementModifier, filter_modify,
};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use std::sync::Arc;

pub struct RandomChancePlacement {
    pub chance: f32,
}

impl RandomChancePlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(RandomChancePlacement {
            chance: fields.required("chance", |v| parse_float_range(v, 0.0, 1.0))?,
        }))
    }
}

impl PlacementFilter for RandomChancePlacement {
    fn should_place(
        &self,
        _context: &PlacementContext<'_>,
        random: &mut dyn RandomSource,
        _origin: BlockPos,
    ) -> bool {
        random.next_float() < self.chance
    }
}

impl PlacementModifier for RandomChancePlacement {
    fn modify(
        &self,
        context: &PlacementContext<'_>,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        output: &mut Vec<BlockPos>,
    ) {
        filter_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static RANDOM_CHANCE: TypeEntry<dyn PlacementModifier> = TypeEntry {
    name: "random_chance",
    decode: RandomChancePlacement::decode,
};
