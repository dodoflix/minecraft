//! `NoiseProvider` (`noise`).

use super::noise_based_state_provider::NoiseBasedStateProvider;
use super::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::codec::{LoadError, LoadResult, Fields, TypeEntry, parse_block_state, parse_list};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_worldgen::synth::NormalNoise;
use std::sync::Arc;

pub struct NoiseProvider {
    pub base: NoiseBasedStateProvider,
    pub states: Vec<BlockState>,
}

impl NoiseProvider {
    pub fn new(seed: i64, parameters: &NormalNoise, scale: f32, states: Vec<BlockState>) -> NoiseProvider {
        NoiseProvider { base: NoiseBasedStateProvider::new(seed, parameters, scale), states }
    }

    /// `getRandomState(states, pos, scale)`.
    pub fn get_random_state_at(&self, states: &[BlockState], pos: BlockPos, scale: f64) -> BlockState {
        let noise_value = self.base.get_noise_value(pos, scale);
        get_random_state(states, noise_value)
    }
}

/// `getRandomState(states, noiseValue)`.
pub fn get_random_state(states: &[BlockState], noise_value: f32) -> BlockState {
    let placement_value = mth::clamp_f32((1.0 + noise_value) / 2.0, 0.0, 0.9999);
    states[(placement_value * states.len() as f32) as i32 as usize]
}

/// `ExtraCodecs.nonEmptyList(BlockState.CODEC.listOf())`.
pub fn non_empty_states(value: &serde_json::Value) -> LoadResult<Vec<BlockState>> {
    let states = parse_list(value, parse_block_state)?;
    if states.is_empty() {
        return Err(LoadError::new("List must have contents"));
    }
    Ok(states)
}

/// `noiseProviderCodec`: `noiseCodec` + `states`.
pub fn noise_provider_codec(fields: &Fields<'_>) -> LoadResult<(i64, NormalNoise, f32, Vec<BlockState>)> {
    let (seed, parameters, scale) = NoiseBasedStateProvider::codec(fields)?;
    let states = fields.required("states", non_empty_states)?;
    Ok((seed, parameters, scale, states))
}

impl BlockStateProvider for NoiseProvider {
    fn get_state(&self, _level: &dyn LevelAccessor, _random: &mut dyn RandomSource, pos: BlockPos) -> BlockState {
        self.get_random_state_at(&self.states, pos, f64::from(self.base.scale))
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static NOISE: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "noise",
    decode: |fields, _| {
        let (seed, parameters, scale, states) = noise_provider_codec(fields)?;
        Ok(Arc::new(NoiseProvider::new(seed, &parameters, scale, states)))
    },
};
