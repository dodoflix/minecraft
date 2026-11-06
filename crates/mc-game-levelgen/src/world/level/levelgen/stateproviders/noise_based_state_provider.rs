//! `NoiseBasedStateProvider`: the shared `seed` / `noise` / `scale` of the noise providers.

use mc_core::pos::BlockPos;
use mc_core::random::{LegacyRandomSource, WorldgenRandom};
use mc_game_core::world::level::levelgen::codec::{
    Fields, LoadResult, parse_long, parse_positive_float,
};
use mc_worldgen::data::parse_noise_parameters;
use mc_worldgen::synth::{Noise, NormalNoise};
use std::sync::Arc;

pub struct NoiseBasedStateProvider {
    pub seed: i64,
    pub scale: f32,
    pub noise: Arc<dyn Noise>,
}

impl NoiseBasedStateProvider {
    /// `noiseCodec`: `seed`, `noise` (`NormalNoise.DIRECT_CODEC`), `scale` (`POSITIVE_FLOAT`).
    pub fn codec(fields: &Fields<'_>) -> LoadResult<(i64, NormalNoise, f32)> {
        let seed = fields.required("seed", parse_long)?;
        let parameters =
            fields.required("noise", |v| parse_noise_parameters(v).map(NormalNoise::new))?;
        let scale = fields.required("scale", parse_positive_float)?;
        Ok((seed, parameters, scale))
    }

    pub fn new(seed: i64, parameters: &NormalNoise, scale: f32) -> NoiseBasedStateProvider {
        NoiseBasedStateProvider {
            seed,
            scale,
            noise: create_noise(parameters, seed),
        }
    }

    /// `getNoiseValue(pos, scale)`.
    pub fn get_noise_value(&self, pos: BlockPos, scale: f64) -> f32 {
        self.noise.get3(
            f64::from(pos.x()) * scale,
            f64::from(pos.y()) * scale,
            f64::from(pos.z()) * scale,
        )
    }
}

/// `parameters.create(new WorldgenRandom(new LegacyRandomSource(seed)))`.
pub fn create_noise(parameters: &NormalNoise, seed: i64) -> Arc<dyn Noise> {
    parameters.create(&mut WorldgenRandom::new(Box::new(LegacyRandomSource::new(
        seed,
    ))))
}
