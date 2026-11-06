//! `NoiseThresholdCountPlacement`: `below_noise` or `above_noise` repetitions depending on the biome info noise.

use super::repeating_placement::{RepeatingPlacement, repeating_modify};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, TypeEntry, parse_f64, parse_int,
};
use mc_game_core::world::level::levelgen::placement::{
    PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier,
};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_worldgen::biome::BIOME_INFO_NOISE;
use mc_worldgen::synth::Noise;
use std::sync::Arc;

pub struct NoiseThresholdCountPlacement {
    pub noise_level: f64,
    pub below_noise: i32,
    pub above_noise: i32,
}

impl NoiseThresholdCountPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(NoiseThresholdCountPlacement {
            noise_level: fields.required("noise_level", parse_f64)?,
            below_noise: fields.required("below_noise", parse_int)?,
            above_noise: fields.required("above_noise", parse_int)?,
        }))
    }
}

impl RepeatingPlacement for NoiseThresholdCountPlacement {
    fn count(&self, _random: &mut dyn RandomSource, origin: BlockPos) -> i32 {
        let flower_noise = f64::from(
            BIOME_INFO_NOISE.get2(f64::from(origin.x()) / 200.0, f64::from(origin.z()) / 200.0),
        );
        if flower_noise < self.noise_level {
            self.below_noise
        } else {
            self.above_noise
        }
    }
}

impl PlacementModifier for NoiseThresholdCountPlacement {
    fn modify(
        &self,
        context: &PlacementContext<'_>,
        random: &mut dyn RandomSource,
        origin: BlockPos,
        output: &mut Vec<BlockPos>,
    ) {
        repeating_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static NOISE_THRESHOLD_COUNT: TypeEntry<dyn PlacementModifier> = TypeEntry {
    name: "noise_threshold_count",
    decode: NoiseThresholdCountPlacement::decode,
};
