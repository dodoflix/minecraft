//! `synth.NormalNoise` (+ `Parameters`, `Normalization`, `Builder`): two offset Perlin octave stacks
//! summed and normalized to a target deviation. The JSON codec lives with the data loader.

use std::sync::Arc;

use mc_core::random::RandomSource;

use super::Noise;
use super::legacy_fbm_initializer;
use super::noise_stack::NoiseStack;
use super::perlin_noise::PerlinNoise;
use crate::util::{Interval, java_pow};

const INPUT_FACTOR: f64 = 1.0181268882175227;
const TARGET_DEVIATION: f64 = 0.3333333333333333;

/// `NormalNoise.Normalization`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Normalization {
    Disabled,
    Enabled,
    Legacy,
}

/// `NormalNoise.Parameters`.
#[derive(Clone, Debug, PartialEq)]
pub struct NoiseParameters {
    pub base_amplitude: f64,
    pub base_octave: i32,
    pub octave_count: i32,
    pub normalize: Normalization,
    pub amplitude_modifiers: Vec<f64>,
}

/// `NormalNoise.OctaveInfo`.
#[derive(Clone, Copy, Debug)]
struct OctaveInfo {
    octave_index: i32,
    frequency: f64,
    amplitude: f64,
}

impl OctaveInfo {
    fn seed(&self) -> String {
        format!("octave_{}", self.octave_index)
    }

    fn abs_amplitude(&self) -> f64 {
        self.amplitude.abs()
    }
}

#[derive(Clone, Debug)]
pub struct NormalNoise {
    parameters: NoiseParameters,
    octaves: Vec<OctaveInfo>,
    normalization_factor: f64,
    range: Interval,
}

/// `DoubleStream.sum()`: Kahan-compensated summation (JDK 17+ `Collectors.sumWithCompensation`).
fn java_double_sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut sum, mut compensation, mut simple_sum) = (0.0f64, 0.0f64, 0.0f64);
    for value in values {
        let tmp = value - compensation;
        let velvel = sum + tmp;
        compensation = (velvel - sum) - tmp;
        sum = velvel;
        simple_sum += value;
    }
    let result = sum - compensation;
    if result.is_nan() && simple_sum.is_infinite() {
        simple_sum
    } else {
        result
    }
}

fn amplitude_modifier(amplitude_modifiers: &[f64], index: i32) -> f64 {
    if amplitude_modifiers.is_empty() {
        1.0
    } else {
        amplitude_modifiers[index as usize]
    }
}

fn build_octaves(
    base_octave: i32,
    base_amplitude: f64,
    octave_count: i32,
    normalize: bool,
    amplitude_modifiers: &[f64],
) -> Vec<OctaveInfo> {
    let mut frequency = java_pow(2.0, base_octave as f64);
    let mut amplitude = base_amplitude;
    if normalize {
        amplitude = base_amplitude
            * (java_pow(0.5, -(octave_count - 1) as f64)
                / (java_pow(0.5, -octave_count as f64) - 1.0));
    }
    let mut octaves = Vec::with_capacity(octave_count.max(0) as usize);
    for i in 0..octave_count {
        let modifier = amplitude_modifier(amplitude_modifiers, i);
        if modifier != 0.0 {
            octaves.push(OctaveInfo {
                octave_index: base_octave + i,
                frequency,
                amplitude: amplitude * modifier,
            });
        }
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    octaves
}

fn compute_normalization_factor(target_amplitude: f64, octaves: &[OctaveInfo]) -> f64 {
    let input_deviation = estimate_deviation(octaves);
    if input_deviation == 0.0 {
        0.0
    } else {
        let input_sum_deviation = input_deviation * 2.0f64.sqrt();
        let target_deviation = target_amplitude * TARGET_DEVIATION;
        target_deviation / input_sum_deviation
    }
}

fn estimate_deviation(octaves: &[OctaveInfo]) -> f64 {
    let mut variance = 0.0;
    for octave in octaves {
        let layer_deviation = PerlinNoise::STANDARD_DEVIATION * octave.abs_amplitude();
        variance += layer_deviation * layer_deviation;
    }
    f64::sqrt(variance)
}

fn compute_parity_base_amplitude(base_octave: i32, amplitudes: &[f64]) -> f64 {
    let octaves = build_octaves(base_octave, 1.0, amplitudes.len() as i32, true, amplitudes);
    let target_amplitude = java_double_sum(octaves.iter().map(OctaveInfo::abs_amplitude));
    let new_normalization_factor = compute_normalization_factor(target_amplitude, &octaves);
    if new_normalization_factor == 0.0 {
        1.0
    } else {
        let old_normalization_factor =
            compute_parity_normalization_factor(1.0, amplitudes.len() as i32, amplitudes);
        old_normalization_factor / new_normalization_factor
    }
}

fn compute_parity_normalization_factor(
    base_amplitude: f64,
    octave_count: i32,
    amplitude_modifiers: &[f64],
) -> f64 {
    let mut min_octave = i32::MAX;
    let mut max_octave = i32::MIN;
    for i in 0..octave_count {
        if amplitude_modifier(amplitude_modifiers, i) != 0.0 {
            min_octave = min_octave.min(i);
            max_octave = max_octave.max(i);
        }
    }
    base_amplitude * 0.5 * 0.3333333333333333
        / parity_expected_deviation(max_octave.wrapping_sub(min_octave))
}

fn parity_expected_deviation(octave_span: i32) -> f64 {
    0.1 * (1.0 + 1.0 / octave_span.wrapping_add(1) as f64)
}

impl NormalNoise {
    /// The private `NormalNoise(Parameters)` constructor.
    pub fn new(parameters: NoiseParameters) -> NormalNoise {
        let octaves = build_octaves(
            parameters.base_octave,
            parameters.base_amplitude,
            parameters.octave_count,
            parameters.normalize != Normalization::Disabled,
            &parameters.amplitude_modifiers,
        );
        let mut target_amplitude = java_double_sum(octaves.iter().map(OctaveInfo::abs_amplitude));
        let mut normalization_factor = compute_normalization_factor(target_amplitude, &octaves);
        if parameters.normalize == Normalization::Legacy && normalization_factor != 0.0 {
            let parity_normalization_factor = compute_parity_normalization_factor(
                parameters.base_amplitude,
                parameters.octave_count,
                &parameters.amplitude_modifiers,
            );
            target_amplitude *= parity_normalization_factor / normalization_factor;
            normalization_factor = parity_normalization_factor;
        }
        let range = Interval::of_symmetric((target_amplitude * TARGET_DEVIATION * 6.0) as f32);
        NormalNoise {
            parameters,
            octaves,
            normalization_factor,
            range,
        }
    }

    pub fn builder() -> NormalNoiseBuilder {
        NormalNoiseBuilder {
            base_amplitude: 1.0,
            base_octave: 0,
            octave_count: 1,
            normalize: Normalization::Enabled,
            amplitude_modifiers: Vec::new(),
        }
    }

    /// `createParity(int, double...)`; panics on empty `amplitudes`.
    pub fn create_parity(first_octave: i32, amplitudes: &[f64]) -> NormalNoise {
        assert!(!amplitudes.is_empty(), "Need at least 1 amplitude");
        let mut parameters = Self::builder();
        parameters.base_octave = first_octave;
        parameters = parameters.set_octave_count(amplitudes.len() as i32);
        parameters.base_amplitude = compute_parity_base_amplitude(first_octave, amplitudes);
        for (i, &modifier) in amplitudes.iter().enumerate() {
            if modifier != 1.0 {
                parameters = parameters.set_amplitude_modifier(i as i32, modifier);
            }
        }
        parameters.build()
    }

    pub fn parameters(&self) -> &NoiseParameters {
        &self.parameters
    }

    pub fn range(&self) -> Interval {
        self.range
    }

    /// `create(RandomSource)`.
    pub fn create(&self, random: &mut dyn RandomSource) -> Arc<dyn Noise> {
        let first_random = random.fork_positional();
        let second_random = random.fork_positional();
        let mut stack = NoiseStack::builder();
        for octave in &self.octaves {
            let octave_seed = octave.seed();
            let first_noise = PerlinNoise::new(&mut *first_random.from_hash_of(&octave_seed));
            let second_noise = PerlinNoise::new(&mut *second_random.from_hash_of(&octave_seed));
            let value_factor = (self.normalization_factor * octave.amplitude) as f32;
            stack = stack.add(Arc::new(first_noise), octave.frequency, value_factor);
            stack = stack.add(
                Arc::new(second_noise),
                octave.frequency * INPUT_FACTOR,
                value_factor,
            );
        }
        Arc::new(stack.build())
    }

    /// `createForLegacyNetherBiome(RandomSource)`.
    pub fn create_for_legacy_nether_biome(&self, random: &mut dyn RandomSource) -> Arc<dyn Noise> {
        let parameters = &self.parameters;
        let ones;
        let amplitudes: &[f64] = if parameters.amplitude_modifiers.is_empty() {
            ones = vec![1.0; parameters.octave_count.max(0) as usize];
            &ones
        } else {
            &parameters.amplitude_modifiers
        };
        let first = legacy_fbm_initializer::create_for_legacy_nether_biome(
            random,
            parameters.base_octave,
            amplitudes,
        );
        let second = legacy_fbm_initializer::create_for_legacy_nether_biome(
            random,
            parameters.base_octave,
            amplitudes,
        );
        let value_factor = (self.normalization_factor * parameters.base_amplitude) as f32;
        Arc::new(
            NoiseStack::builder()
                .add_stack(&first, 1.0, value_factor)
                .add_stack(&second, INPUT_FACTOR, value_factor)
                .build(),
        )
    }
}

/// `NormalNoise.Builder`.
pub struct NormalNoiseBuilder {
    base_amplitude: f64,
    base_octave: i32,
    octave_count: i32,
    normalize: Normalization,
    amplitude_modifiers: Vec<f64>,
}

impl NormalNoiseBuilder {
    pub fn set_base_amplitude(mut self, base_amplitude: f64) -> Self {
        self.base_amplitude = base_amplitude;
        self
    }

    pub fn set_base_octave(mut self, base_octave: i32) -> Self {
        self.base_octave = base_octave;
        self
    }

    /// Panics if an amplitude modifier was already set.
    pub fn set_octave_count(mut self, octave_count: i32) -> Self {
        assert!(
            self.amplitude_modifiers.is_empty(),
            "Cannot set octave count after setting amplitude modifier"
        );
        self.octave_count = octave_count;
        self
    }

    pub fn set_normalize(mut self, normalize: bool) -> Self {
        self.normalize = if normalize {
            Normalization::Enabled
        } else {
            Normalization::Disabled
        };
        self
    }

    pub fn set_legacy_normalization(mut self) -> Self {
        self.normalize = Normalization::Legacy;
        self
    }

    /// Panics if `index` is outside `[0, octave_count)`.
    pub fn set_amplitude_modifier(mut self, index: i32, value: f64) -> Self {
        assert!(
            index >= 0 && index < self.octave_count,
            "{index} outside of octave range [0; {})",
            self.octave_count
        );
        if self.amplitude_modifiers.is_empty() {
            self.amplitude_modifiers = vec![1.0; self.octave_count as usize];
        }
        self.amplitude_modifiers[index as usize] = value;
        self
    }

    pub fn build(self) -> NormalNoise {
        NormalNoise::new(NoiseParameters {
            base_amplitude: self.base_amplitude,
            base_octave: self.base_octave,
            octave_count: self.octave_count,
            normalize: self.normalize,
            amplitude_modifiers: self.amplitude_modifiers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vanilla oracle: `DoubleStream.of(1e16, 1.0, 1.0, -1e16).sum()` is 2.0 (a naive sum gives 0.0).
    #[test]
    fn double_sum_is_compensated() {
        assert_eq!(java_double_sum([1e16, 1.0, 1.0, -1e16].into_iter()), 2.0);
    }

    #[test]
    #[should_panic(expected = "Cannot set octave count after setting amplitude modifier")]
    fn octave_count_after_modifier_panics() {
        NormalNoise::builder()
            .set_octave_count(2)
            .set_amplitude_modifier(0, 2.0)
            .set_octave_count(3);
    }

    #[test]
    #[should_panic(expected = "2 outside of octave range [0; 2)")]
    fn amplitude_modifier_out_of_range_panics() {
        NormalNoise::builder()
            .set_octave_count(2)
            .set_amplitude_modifier(2, 1.0);
    }
}
