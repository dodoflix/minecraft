//! `synth.LegacyFbmInitializer`: the pre-1.18 octave layout used by the legacy nether biome noises.

use std::sync::Arc;

use mc_core::random::RandomSource;

use super::noise_stack::NoiseStack;
use super::perlin_noise::PerlinNoise;
use crate::util::java_pow;

/// `createForLegacyNetherBiome`; panics where vanilla throws.
pub fn create_for_legacy_nether_biome(
    random: &mut dyn RandomSource,
    first_octave: i32,
    amplitudes: &[f64],
) -> NoiseStack {
    let octaves = amplitudes.len() as i32;
    let zero_octave_index = -first_octave;
    let mut noise_levels: Vec<Option<Arc<PerlinNoise>>> = vec![None; amplitudes.len()];
    let zero_octave = Arc::new(PerlinNoise::new(random));
    if (0..octaves).contains(&zero_octave_index) && amplitudes[zero_octave_index as usize] != 0.0 {
        noise_levels[zero_octave_index as usize] = Some(zero_octave);
    }

    for i in (0..zero_octave_index).rev() {
        if i < octaves && amplitudes[i as usize] != 0.0 {
            noise_levels[i as usize] = Some(Arc::new(PerlinNoise::new(random)));
        } else {
            skip_octave(random);
        }
    }

    let created = noise_levels.iter().flatten().count();
    let non_zero = amplitudes.iter().filter(|&&a| a != 0.0).count();
    assert!(
        created == non_zero,
        "Failed to create correct number of noise levels for given non-zero amplitudes"
    );
    assert!(
        zero_octave_index >= octaves - 1,
        "Positive octaves are temporarily disabled"
    );

    let mut factor = java_pow(2.0, -zero_octave_index as f64);
    let mut value_factor =
        java_pow(2.0, (octaves - 1) as f64) / (java_pow(2.0, octaves as f64) - 1.0);
    let mut stack = NoiseStack::builder();
    for (noise, amplitude) in noise_levels.into_iter().zip(amplitudes) {
        if let Some(noise) = noise {
            stack = stack.add(noise, factor, (value_factor * amplitude) as f32);
        }
        factor *= 2.0;
        value_factor /= 2.0;
    }
    stack.build()
}

fn skip_octave(random: &mut dyn RandomSource) {
    random.consume_count(262);
}
