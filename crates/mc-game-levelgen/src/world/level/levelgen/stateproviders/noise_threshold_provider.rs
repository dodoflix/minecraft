//! `NoiseThresholdProvider` (`noise_threshold`).

use super::noise_based_state_provider::NoiseBasedStateProvider;
use super::noise_provider::non_empty_states;
use mc_game_core::world::level::levelgen::stateproviders::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_block_state, parse_float_range};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct NoiseThresholdProvider {
    pub base: NoiseBasedStateProvider,
    pub threshold: f32,
    pub high_chance: f32,
    pub default_state: BlockState,
    pub low_states: Vec<BlockState>,
    pub high_states: Vec<BlockState>,
}

/// `Util.getRandom(list, random)`.
fn get_random(list: &[BlockState], random: &mut dyn RandomSource) -> BlockState {
    list[random.next_int_bounded(list.len() as i32) as usize]
}

impl BlockStateProvider for NoiseThresholdProvider {
    fn get_state(&self, _level: &dyn LevelAccessor, random: &mut dyn RandomSource, pos: BlockPos) -> BlockState {
        let local_value = f64::from(self.base.get_noise_value(pos, f64::from(self.base.scale)));
        if local_value < f64::from(self.threshold) {
            get_random(&self.low_states, random)
        } else if random.next_float() < self.high_chance {
            get_random(&self.high_states, random)
        } else {
            self.default_state
        }
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static NOISE_THRESHOLD: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "noise_threshold",
    decode: |fields, _| {
        let (seed, parameters, scale) = NoiseBasedStateProvider::codec(fields)?;
        let threshold = fields.required("threshold", |v| parse_float_range(v, -1.0, 1.0))?;
        let high_chance = fields.required("high_chance", |v| parse_float_range(v, 0.0, 1.0))?;
        let default_state = fields.required("default_state", parse_block_state)?;
        let low_states = fields.required("low_states", non_empty_states)?;
        let high_states = fields.required("high_states", non_empty_states)?;
        Ok(Arc::new(NoiseThresholdProvider {
            base: NoiseBasedStateProvider::new(seed, &parameters, scale),
            threshold,
            high_chance,
            default_state,
            low_states,
            high_states,
        }))
    },
};
