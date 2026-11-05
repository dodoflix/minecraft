//! `net.minecraft.world.level.block.NetherVines` (shared by weeping and twisting vines).
use super::prelude::*;
use mc_core::random::RandomSource;

/// `NetherVines.GROW_PER_TICK_PROBABILITY`.
pub const GROW_PER_TICK_PROBABILITY: f64 = 0.1;
const BONEMEAL_GROW_PROBABILITY_DECREASE_RATE: f64 = 0.826;

/// `NetherVines.isValidGrowthState`.
pub fn is_valid_growth_state(state: BlockState) -> bool {
    state.is_air()
}

/// `NetherVines.getBlocksToGrowWhenBonemealed`.
pub fn get_blocks_to_grow_when_bonemealed(random: &mut dyn RandomSource) -> i32 {
    let mut grow_probability = 1.0;
    let mut count = 0;
    while random.next_double() < grow_probability {
        grow_probability *= BONEMEAL_GROW_PROBABILITY_DECREASE_RATE;
        count += 1;
    }
    count
}
