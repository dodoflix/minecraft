//! `op/PowFunction.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::op::unary::{
    CubeSampler, ReciprocalSampler, SqrtSampler, SquareSampler, map_buffer,
};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, java_pow};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PowFunction {
    pub base: DensityFunction,
    pub exponent: DensityFunction,
}

/// `(float) Math.pow(base, exponent)` on float operands.
#[inline]
fn pow_f32(base: f64, exponent: f64) -> f32 {
    java_pow(base, exponent) as f32
}

impl PowFunction {
    /// `PowFunction.compileConstExponent`.
    fn compile_const_exponent(base: SharedSampler, exponent: f32) -> SharedSampler {
        let abs_exponent = exponent.abs();
        let special_sampler: SharedSampler = if abs_exponent == 0.5 {
            Arc::new(SqrtSampler::new(base))
        } else if abs_exponent == 1.0 {
            base
        } else if abs_exponent == 2.0 {
            Arc::new(SquareSampler::new(base))
        } else if abs_exponent == 3.0 {
            Arc::new(CubeSampler::new(base))
        } else {
            return Arc::new(ConstExponentSampler::new(base, exponent as f64));
        };
        if exponent >= 0.0 {
            special_sampler
        } else {
            Arc::new(ReciprocalSampler::new(special_sampler))
        }
    }
}

impl DfOp for PowFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let base = self.base.compile_sampler(ctx);
        let exponent = self.exponent.compile_sampler(ctx);
        if let DfNode::Constant(constant) = self.base.node() {
            return Arc::new(ConstBaseSampler::new(constant.value as f64, exponent));
        }
        if let DfNode::Constant(constant) = self.exponent.node() {
            return Self::compile_const_exponent(base, constant.value);
        }
        Arc::new(PowSampler::new(base, exponent))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let base = rule.rewrite(&self.base);
        let exponent = rule.rewrite(&self.exponent);
        if base.ptr_eq(&self.base) && exponent.ptr_eq(&self.exponent) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Pow(PowFunction { base, exponent }))
        }
    }
    fn range(&self) -> Interval {
        Interval::pow(self.base.range(), self.exponent.range())
    }
    fn domain_axes(&self) -> u8 {
        self.base.domain_axes() | self.exponent.domain_axes()
    }
}

/// `PowFunction.ConstBaseSampler`.
struct ConstBaseSampler {
    base: f64,
    exponent: SharedSampler,
}

impl ConstBaseSampler {
    fn new(base: f64, exponent: SharedSampler) -> ConstBaseSampler {
        ConstBaseSampler { base, exponent }
    }
}

impl DensitySampler for ConstBaseSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.exponent.sample_volume(context, out, volume);
        map_buffer(out, |exponent| pow_f32(self.base, exponent as f64));
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        pow_f32(
            self.base,
            self.exponent
                .sample_value(context, block_x, block_y, block_z) as f64,
        )
    }
}

/// `PowFunction.ConstExponentSampler`.
struct ConstExponentSampler {
    base: SharedSampler,
    exponent: f64,
}

impl ConstExponentSampler {
    fn new(base: SharedSampler, exponent: f64) -> ConstExponentSampler {
        ConstExponentSampler { base, exponent }
    }
}

impl DensitySampler for ConstExponentSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.base.sample_volume(context, out, volume);
        map_buffer(out, |base| pow_f32(base as f64, self.exponent));
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        pow_f32(
            self.base.sample_value(context, block_x, block_y, block_z) as f64,
            self.exponent,
        )
    }
}

/// `PowFunction.Sampler`.
struct PowSampler {
    base: SharedSampler,
    exponent: SharedSampler,
}

impl PowSampler {
    fn new(base: SharedSampler, exponent: SharedSampler) -> PowSampler {
        PowSampler { base, exponent }
    }
}

impl DensitySampler for PowSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.base.sample_volume(context, out, volume);

        let mut exponent_buffer = context.acquire_buffer(volume);
        self.exponent
            .sample_volume(context, &mut exponent_buffer, volume);

        for i in 0..out.size() {
            out.set(i, pow_f32(out.get(i) as f64, exponent_buffer.get(i) as f64));
        }
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let base = self.base.sample_value(context, block_x, block_y, block_z);
        let exponent = self
            .exponent
            .sample_value(context, block_x, block_y, block_z);
        pow_f32(base as f64, exponent as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::generator::constant::tests::{NoCtx, assert_volume_matches_value};

    /// `y_clamped_gradient(0, 8, 0, 8)`: value `y` clamped to `[0, 8]`.
    fn ramp() -> DensityFunction {
        DensityFunction::y_clamped_gradient(0, 8, 0.0, 8.0)
    }

    fn at(function: &DensityFunction, y: i32) -> f32 {
        function
            .compile_sampler(&NoCtx)
            .sample_value(&SamplerContext::empty_uncached(), 0, y, 0)
    }

    #[test]
    fn const_exponent_special_cases() {
        assert_eq!(at(&ramp().pow(DensityFunction::constant(0.5)), 4), 2.0);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(-0.5)), 4), 0.5);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(1.0)), 5), 5.0);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(-1.0)), 4), 0.25);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(2.0)), 3), 9.0);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(-2.0)), 2), 0.25);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(3.0)), 3), 27.0);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(-3.0)), 2), 0.125);
        // -0.0 >= 0 selects the non-reciprocal branch; zero exponent goes through Math.pow
        assert_eq!(at(&ramp().pow(DensityFunction::constant(-0.0)), 0), 1.0);
        assert_eq!(at(&ramp().pow(DensityFunction::constant(1.5)), 4), 8.0);
    }

    #[test]
    fn negative_base() {
        let negative = DensityFunction::y_clamped_gradient(0, 1, -2.0, -2.0);
        assert_eq!(
            at(&negative.clone().pow(DensityFunction::constant(2.0)), 0),
            4.0
        );
        assert!(at(&negative.clone().pow(DensityFunction::constant(0.5)), 0).is_nan());
        assert!(at(&negative.pow(DensityFunction::constant(1.5)), 0).is_nan());
    }

    #[test]
    fn const_base_and_generic() {
        assert_eq!(at(&DensityFunction::constant(2.0).pow(ramp()), 3), 8.0);
        assert_eq!(
            at(
                &DensityFunction::constant(4.0).pow(DensityFunction::constant(0.5)),
                0
            ),
            2.0
        );
        assert_eq!(at(&ramp().pow(ramp()), 3), 27.0);
        // Math.pow(x, 0) == 1 even for NaN x; pow(1, NaN) is NaN in Java
        let nan = DensityFunction::constant(f32::NAN);
        assert_eq!(at(&nan.clone().abs().pow(ramp()), 0), 1.0);
        assert!(at(&DensityFunction::constant(1.0).abs().pow(nan.abs()), 0).is_nan());
    }

    #[test]
    fn volume_path_matches_point_path() {
        for exponent in [0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 1.7] {
            assert_volume_matches_value(
                &*ramp()
                    .pow(DensityFunction::constant(exponent))
                    .compile_sampler(&NoCtx),
            );
        }
        assert_volume_matches_value(
            &*DensityFunction::constant(2.0)
                .pow(ramp())
                .compile_sampler(&NoCtx),
        );
        assert_volume_matches_value(&*ramp().pow(ramp()).compile_sampler(&NoCtx));
    }

    #[test]
    fn range_and_axes() {
        let function = ramp().pow(DensityFunction::constant(2.0));
        assert_eq!(function.range(), Interval::of(0.0, 64.0));
        assert_eq!(function.domain_axes(), 2);
    }
}
