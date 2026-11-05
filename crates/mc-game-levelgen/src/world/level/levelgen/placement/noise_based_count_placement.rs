//! `NoiseBasedCountPlacement`: `ceil((noise(x / factor, z / factor) + offset) * ratio)` repetitions.

use super::repeating_placement::{RepeatingPlacement, repeating_modify};
use super::{PLACEMENT_MODIFIER_TYPES, PlacementContext, PlacementModifier};
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_f64, parse_int};
use crate::world::level::levelgen::registries::DecodeCtx;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::biome::BIOME_INFO_NOISE;
use mc_worldgen::synth::Noise;
use std::sync::Arc;

pub struct NoiseBasedCountPlacement {
    pub noise_to_count_ratio: i32,
    pub noise_factor: f64,
    pub noise_offset: f64,
}

impl NoiseBasedCountPlacement {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn PlacementModifier>> {
        Ok(Arc::new(NoiseBasedCountPlacement {
            noise_to_count_ratio: fields.required("noise_to_count_ratio", parse_int)?,
            noise_factor: fields.required("noise_factor", parse_f64)?,
            noise_offset: fields.optional("noise_offset", parse_f64)?.unwrap_or(0.0),
        }))
    }
}

impl RepeatingPlacement for NoiseBasedCountPlacement {
    fn count(&self, _random: &mut dyn RandomSource, origin: BlockPos) -> i32 {
        let flower_noise = f64::from(BIOME_INFO_NOISE.get2(f64::from(origin.x()) / self.noise_factor, f64::from(origin.z()) / self.noise_factor));
        ((flower_noise + self.noise_offset) * f64::from(self.noise_to_count_ratio)).ceil() as i32
    }
}

impl PlacementModifier for NoiseBasedCountPlacement {
    fn modify(&self, context: &PlacementContext<'_>, random: &mut dyn RandomSource, origin: BlockPos, output: &mut Vec<BlockPos>) {
        repeating_modify(self, context, random, origin, output);
    }
}

#[linkme::distributed_slice(PLACEMENT_MODIFIER_TYPES)]
static NOISE_BASED_COUNT: TypeEntry<dyn PlacementModifier> =
    TypeEntry { name: "noise_based_count", decode: NoiseBasedCountPlacement::decode };
