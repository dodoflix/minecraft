//! `synth.BlendedNoise`: the pre-1.18 terrain noise (density function type `old_blended_noise`).

use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock};

use mc_core::ident::Identifier;
use mc_core::random::RandomSource;

use super::noise_stack::NoiseStack;
use super::smeared_perlin_noise::SmearedPerlinNoise;
use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfOp};
use crate::densityfunction::generator::noise_function::NoiseSampler;
use crate::densityfunction::op::binary::ConstAddSampler;
use crate::densityfunction::op::clamp::ClampSampler;
use crate::densityfunction::op::lerp::LerpSampler;
use crate::densityfunction::sampler::SharedSampler;
use crate::util::{Interval, java_pow};

const BASE_SCALE: f64 = 684.412;
/// `(double) 0.99998474F`.
const LIMIT_FACTOR: f64 = 0.99998474f32 as f64;
const MAIN_FACTOR: f64 = 12.75;
const LIMIT_FIRST_OCTAVE: i32 = -15;
const MAIN_FIRST_OCTAVE: i32 = -7;

/// `BlendedNoise.NOISE_SEED`.
pub static NOISE_SEED: LazyLock<Identifier> = LazyLock::new(|| {
    Identifier::with_default_namespace("terrain").expect("`terrain` is a valid identifier path")
});

/// `BlendedNoise.FbmSet`.
pub struct FbmSet {
    pub min_limit_noise: NoiseStack,
    pub max_limit_noise: NoiseStack,
    pub main_noise: NoiseStack,
}

#[derive(Clone, Debug)]
pub struct BlendedNoise {
    pub xz_scale: f64,
    pub y_scale: f64,
    pub xz_factor: f64,
    pub y_factor: f64,
    pub smear_scale_multiplier: f64,
}
impl PartialEq for BlendedNoise {
    fn eq(&self, o: &Self) -> bool {
        self.xz_scale.to_bits() == o.xz_scale.to_bits()
            && self.y_scale.to_bits() == o.y_scale.to_bits()
            && self.xz_factor.to_bits() == o.xz_factor.to_bits()
            && self.y_factor.to_bits() == o.y_factor.to_bits()
            && self.smear_scale_multiplier.to_bits() == o.smear_scale_multiplier.to_bits()
    }
}
impl Eq for BlendedNoise {}
impl Hash for BlendedNoise {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.xz_scale.to_bits().hash(s);
        self.y_scale.to_bits().hash(s);
        self.xz_factor.to_bits().hash(s);
        self.y_factor.to_bits().hash(s);
        self.smear_scale_multiplier.to_bits().hash(s);
    }
}
impl BlendedNoise {
    fn xz_multiplier(&self) -> f64 {
        BASE_SCALE * self.xz_scale
    }

    fn y_multiplier(&self) -> f64 {
        BASE_SCALE * self.y_scale
    }

    pub fn create_fbm_set(&self, random: &mut dyn RandomSource) -> FbmSet {
        let limit_smear_scale_y = self.y_multiplier() * self.smear_scale_multiplier;
        let main_smear_scale_y = limit_smear_scale_y / self.y_factor;
        FbmSet {
            min_limit_noise: create_fbm(
                random,
                LIMIT_FIRST_OCTAVE,
                limit_smear_scale_y,
                LIMIT_FACTOR,
            ),
            max_limit_noise: create_fbm(
                random,
                LIMIT_FIRST_OCTAVE,
                limit_smear_scale_y,
                LIMIT_FACTOR,
            ),
            main_noise: create_fbm(random, MAIN_FIRST_OCTAVE, main_smear_scale_y, MAIN_FACTOR),
        }
    }

    /// The `@VisibleForTesting` `compileSampler(RandomSource)` overload.
    pub fn compile_sampler_with_random(&self, random: &mut dyn RandomSource) -> SharedSampler {
        let fbms = self.create_fbm_set(random);
        let xz_multiplier = self.xz_multiplier();
        let y_multiplier = self.y_multiplier();
        let min_limit_noise =
            NoiseSampler::new(Arc::new(fbms.min_limit_noise), xz_multiplier, y_multiplier);
        let max_limit_noise =
            NoiseSampler::new(Arc::new(fbms.max_limit_noise), xz_multiplier, y_multiplier);
        let main_noise = NoiseSampler::new(
            Arc::new(fbms.main_noise),
            xz_multiplier / self.xz_factor,
            y_multiplier / self.y_factor,
        );
        let choice = ClampSampler::new(
            Arc::new(ConstAddSampler::new(Arc::new(main_noise), 0.5)),
            0.0,
            1.0,
        );
        Arc::new(LerpSampler::new(
            Arc::new(choice),
            Arc::new(min_limit_noise),
            Arc::new(max_limit_noise),
        ))
    }
}

/// `BlendedNoise.createFbm`; panics if `first_octave > 0`.
pub fn create_fbm(
    random: &mut dyn RandomSource,
    first_octave: i32,
    smear_scale_y: f64,
    mut value_factor: f64,
) -> NoiseStack {
    assert!(first_octave <= 0, "firstOctave>0");
    let octaves = -first_octave + 1;
    let mut factor = 1.0;
    value_factor /= java_pow(2.0, octaves as f64) - 1.0;
    let mut stack = NoiseStack::builder();
    for _ in 0..octaves {
        stack = stack.add(
            Arc::new(SmearedPerlinNoise::new(random, smear_scale_y * factor)),
            factor,
            value_factor as f32,
        );
        factor /= 2.0;
        value_factor *= 2.0;
    }
    stack.build()
}

fn compute_fbm_range(first_octave: i32, smear_scale_y: f64, mut value_factor: f64) -> Interval {
    let octaves = -first_octave + 1;
    let mut factor = 1.0;
    value_factor /= java_pow(2.0, octaves as f64) - 1.0;
    let mut range = Interval::of_exact(0.0);
    for _ in 0..octaves {
        let layer_range = Interval::mul(
            SmearedPerlinNoise::range_for(smear_scale_y * factor),
            Interval::of_exact(value_factor as f32),
        );
        range = Interval::add(range, layer_range);
        factor /= 2.0;
        value_factor *= 2.0;
    }
    range
}

impl DfOp for BlendedNoise {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        self.compile_sampler_with_random(&mut *ctx.create_random(&NOISE_SEED))
    }

    fn rewrite_children(
        &self,
        this: &DensityFunction,
        _rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        this.clone()
    }

    fn range(&self) -> Interval {
        compute_fbm_range(
            LIMIT_FIRST_OCTAVE,
            self.y_multiplier() * self.smear_scale_multiplier,
            LIMIT_FACTOR,
        )
    }

    fn domain_axes(&self) -> u8 {
        7
    }
}
