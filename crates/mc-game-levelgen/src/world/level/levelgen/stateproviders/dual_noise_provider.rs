//! `DualNoiseProvider` (`dual_noise`).

use super::noise_based_state_provider::create_noise;
use super::noise_provider::{NoiseProvider, get_random_state, noise_provider_codec};
use mc_game_core::world::level::levelgen::stateproviders::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_int, parse_list, parse_positive_float};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_worldgen::data::parse_noise_parameters;
use mc_worldgen::synth::{Noise, NormalNoise};
use serde_json::Value;
use std::sync::Arc;

pub struct DualNoiseProvider {
    pub noise_provider: NoiseProvider,
    /// `InclusiveRange<Integer>` `variety`.
    pub variety: (i32, i32),
    pub slow_scale: f32,
    slow_noise: Arc<dyn Noise>,
}

impl DualNoiseProvider {
    pub fn new(
        variety: (i32, i32),
        slow_noise_parameters: &NormalNoise,
        slow_scale: f32,
        seed: i64,
        parameters: &NormalNoise,
        scale: f32,
        states: Vec<BlockState>,
    ) -> DualNoiseProvider {
        DualNoiseProvider {
            noise_provider: NoiseProvider::new(seed, parameters, scale, states),
            variety,
            slow_scale,
            slow_noise: create_noise(slow_noise_parameters, seed),
        }
    }

    /// `getSlowNoiseValue(pos)`: the coordinates are scaled in `float` arithmetic.
    fn get_slow_noise_value(&self, pos: BlockPos) -> f32 {
        let s = self.slow_scale;
        self.slow_noise.get3(f64::from(pos.x() as f32 * s), f64::from(pos.y() as f32 * s), f64::from(pos.z() as f32 * s))
    }
}

impl BlockStateProvider for DualNoiseProvider {
    fn get_state(&self, _level: &dyn LevelAccessor, _random: &mut dyn RandomSource, pos: BlockPos) -> BlockState {
        let variety_noise = f64::from(self.get_slow_noise_value(pos));
        let local_variety = mth::clamped_map_f64(variety_noise, -1.0, 1.0, f64::from(self.variety.0), f64::from(self.variety.1.wrapping_add(1))) as i32;
        let possible_states: Vec<BlockState> = (0..local_variety)
            .map(|i| {
                let slow = self.get_slow_noise_value(pos.offset(i.wrapping_mul(54545), 0, i.wrapping_mul(34234)));
                get_random_state(&self.noise_provider.states, slow)
            })
            .collect();
        self.noise_provider.get_random_state_at(&possible_states, pos, f64::from(self.noise_provider.base.scale))
    }
}

/// `InclusiveRange.codec(Codec.INT, min, max)`: an int (a one-value range), `[min, max]` or `{min_inclusive, max_inclusive}`.
fn parse_inclusive_int_range(value: &Value, min_allowed: i32, max_allowed: i32) -> LoadResult<(i32, i32)> {
    fn create(min: i32, max: i32) -> LoadResult<(i32, i32)> {
        if min <= max { Ok((min, max)) } else { Err(LoadError::new("min_inclusive must be less than or equal to max_inclusive")) }
    }
    let (min, max) = match value {
        Value::Number(_) => {
            let v = parse_int(value)?;
            (v, v)
        }
        Value::Array(_) => {
            let list = parse_list(value, parse_int)?;
            match list[..] {
                [min, max] => create(min, max)?,
                _ => return Err(LoadError::new("Input is not a list of 2 elements")),
            }
        }
        _ => {
            let fields = Fields::new(value)?;
            let min = fields.required("min_inclusive", parse_int)?;
            let max = fields.required("max_inclusive", parse_int)?;
            create(min, max)?
        }
    };
    if min < min_allowed {
        return Err(LoadError::new(format!("Range limit too low, expected at least {min_allowed} [{min}-{max}]")));
    }
    if max > max_allowed {
        return Err(LoadError::new(format!("Range limit too high, expected at most {max_allowed} [{min}-{max}]")));
    }
    Ok((min, max))
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static DUAL_NOISE: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "dual_noise",
    decode: |fields, _| {
        let variety = fields.required("variety", |v| parse_inclusive_int_range(v, 1, 64))?;
        let slow_noise = fields.required("slow_noise", |v| parse_noise_parameters(v).map(NormalNoise::new))?;
        let slow_scale = fields.required("slow_scale", parse_positive_float)?;
        let (seed, parameters, scale, states) = noise_provider_codec(fields)?;
        Ok(Arc::new(DualNoiseProvider::new(variety, &slow_noise, slow_scale, seed, &parameters, scale, states)))
    },
};
