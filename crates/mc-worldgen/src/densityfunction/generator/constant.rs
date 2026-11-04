//! `generator/ConstantFunction.java`.
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::Interval;

#[derive(Clone, Debug)]
pub struct ConstantFunction {
    pub value: f32,
}
impl PartialEq for ConstantFunction {
    fn eq(&self, o: &Self) -> bool {
        self.value.to_bits() == o.value.to_bits()
    }
}
impl Eq for ConstantFunction {}
impl Hash for ConstantFunction {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.value.to_bits().hash(s)
    }
}

impl DfOp for ConstantFunction {
    fn compile_sampler(&self, _ctx: &dyn CompileContext) -> SharedSampler {
        Arc::new(ConstantSampler::new(self.value))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        _rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        this.clone()
    }
    fn range(&self) -> Interval {
        Interval::of_exact(self.value)
    }
    fn domain_axes(&self) -> u8 {
        0
    }
}

/// `ConstantFunction.Sampler`.
#[derive(Clone, Copy, Debug)]
pub struct ConstantSampler {
    value: f32,
}

impl ConstantSampler {
    pub fn new(value: f32) -> ConstantSampler {
        ConstantSampler { value }
    }
}

impl DensitySampler for ConstantSampler {
    fn sample_volume(
        &self,
        _context: &SamplerContext,
        out: &mut DensityBuffer,
        _volume: &DensityVolume,
    ) {
        out.fill(self.value);
    }
    fn sample_value(
        &self,
        _context: &SamplerContext,
        _block_x: i32,
        _block_y: i32,
        _block_z: i32,
    ) -> f32 {
        self.value
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::densityfunction::NoiseHolder;
    use crate::synth::Noise;
    use mc_core::ident::Identifier;
    use mc_core::random::RandomSource;

    /// Compile context for functions that need no noise or random source.
    pub(crate) struct NoCtx;
    impl CompileContext for NoCtx {
        fn create_noise_sampler(&self, _: &NoiseHolder) -> Arc<dyn Noise> {
            unreachable!("test does not use noise")
        }
        fn create_random(&self, _: &Identifier) -> Box<dyn RandomSource> {
            unreachable!("test does not use random")
        }
        fn create_end_island_random(&self) -> Box<dyn RandomSource> {
            unreachable!("test does not use random")
        }
    }

    /// Asserts that the volume path of `sampler` equals its point path bit for bit on a small volume.
    pub(crate) fn assert_volume_matches_value(sampler: &dyn DensitySampler) {
        let ctx = SamplerContext::empty_uncached();
        let volume = DensityVolume::new(3, 17, 2, -1, -8, 4, 2, 1, 3);
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&ctx, &mut out, &volume);
        for z in 0..volume.size_z {
            for x in 0..volume.size_x {
                for y in 0..volume.size_y {
                    let expected = sampler.sample_value(
                        &ctx,
                        volume.block_x(x),
                        volume.block_y(y),
                        volume.block_z(z),
                    );
                    let actual = out.get(volume.index_unchecked(x, y, z));
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "at ({x},{y},{z}): {actual} vs {expected}"
                    );
                }
            }
        }
    }

    /// `y_clamped_gradient(-8, 8, -4, 4)` compiled: value `y / 2` for y in `[-8, 8]`.
    pub(crate) fn y_ramp() -> SharedSampler {
        DensityFunction::y_clamped_gradient(-8, 8, -4.0, 4.0).compile_sampler(&NoCtx)
    }

    #[test]
    fn constant_fills_volume_and_has_exact_range() {
        let function = ConstantFunction { value: 2.5 };
        let sampler = function.compile_sampler(&NoCtx);
        let ctx = SamplerContext::empty_uncached();
        let volume = DensityVolume::with_unit_step(2, 3, 2, 0, 0, 0);
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&ctx, &mut out, &volume);
        assert!(out.as_slice().iter().all(|&v| v == 2.5));
        assert_eq!(sampler.sample_value(&ctx, 5, -9, 1), 2.5);
        assert_eq!(function.range(), Interval::of_exact(2.5));
        assert_eq!(function.domain_axes(), 0);
    }

    #[test]
    fn equality_is_bitwise() {
        assert_ne!(
            ConstantFunction { value: 0.0 },
            ConstantFunction { value: -0.0 }
        );
        assert_eq!(
            ConstantFunction { value: 1.0 },
            ConstantFunction { value: 1.0 }
        );
    }
}
