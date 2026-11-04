//! `op/LerpFunction.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, mth};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LerpFunction {
    pub alpha: DensityFunction,
    pub first: DensityFunction,
    pub second: DensityFunction,
}

impl DfOp for LerpFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let alpha = self.alpha.compile_sampler(ctx);
        let first = self.first.compile_sampler(ctx);
        let second = self.second.compile_sampler(ctx);
        if let DfNode::Constant(constant) = self.first.node() {
            return Arc::new(ConstFirstSampler::new(alpha, constant.value, second));
        }
        if let DfNode::Constant(constant) = self.second.node() {
            return Arc::new(ConstSecondSampler::new(alpha, first, constant.value));
        }
        Arc::new(LerpSampler::new(alpha, first, second))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let alpha = rule.rewrite(&self.alpha);
        let first = rule.rewrite(&self.first);
        let second = rule.rewrite(&self.second);
        if alpha.ptr_eq(&self.alpha) && first.ptr_eq(&self.first) && second.ptr_eq(&self.second) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Lerp(LerpFunction {
                alpha,
                first,
                second,
            }))
        }
    }
    fn range(&self) -> Interval {
        Interval::lerp(self.alpha.range(), self.first.range(), self.second.range())
    }
    fn domain_axes(&self) -> u8 {
        self.alpha.domain_axes() | self.first.domain_axes() | self.second.domain_axes()
    }
}

/// The per-element lerp of the volume paths: `alpha == 0` and `alpha == 1` select an end exactly.
#[inline]
fn lerp_element(alpha: f32, first: f32, second: f32) -> f32 {
    if alpha == 0.0 {
        first
    } else if alpha == 1.0 {
        second
    } else {
        mth::lerp(alpha, first, second)
    }
}

/// `LerpFunction.ConstFirstSampler`.
pub struct ConstFirstSampler {
    alpha: SharedSampler,
    first: f32,
    second: SharedSampler,
}

impl ConstFirstSampler {
    pub fn new(alpha: SharedSampler, first: f32, second: SharedSampler) -> ConstFirstSampler {
        ConstFirstSampler {
            alpha,
            first,
            second,
        }
    }
}

impl DensitySampler for ConstFirstSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.alpha.sample_volume(context, out, volume);

        let mut second_buffer = context.acquire_buffer(volume);
        self.second
            .sample_volume(context, &mut second_buffer, volume);

        for i in 0..out.size() {
            out.set(
                i,
                lerp_element(out.get(i), self.first, second_buffer.get(i)),
            );
        }
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let alpha = self.alpha.sample_value(context, block_x, block_y, block_z);
        if alpha == 0.0 {
            self.first
        } else if alpha == 1.0 {
            self.second.sample_value(context, block_x, block_y, block_z)
        } else {
            mth::lerp(
                alpha,
                self.first,
                self.second.sample_value(context, block_x, block_y, block_z),
            )
        }
    }
}

/// `LerpFunction.ConstSecondSampler`.
pub struct ConstSecondSampler {
    alpha: SharedSampler,
    first: SharedSampler,
    second: f32,
}

impl ConstSecondSampler {
    pub fn new(alpha: SharedSampler, first: SharedSampler, second: f32) -> ConstSecondSampler {
        ConstSecondSampler {
            alpha,
            first,
            second,
        }
    }
}

impl DensitySampler for ConstSecondSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.alpha.sample_volume(context, out, volume);

        let mut first_buffer = context.acquire_buffer(volume);
        self.first.sample_volume(context, &mut first_buffer, volume);

        for i in 0..out.size() {
            out.set(
                i,
                lerp_element(out.get(i), first_buffer.get(i), self.second),
            );
        }
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let alpha = self.alpha.sample_value(context, block_x, block_y, block_z);
        if alpha == 0.0 {
            self.first.sample_value(context, block_x, block_y, block_z)
        } else if alpha == 1.0 {
            self.second
        } else {
            mth::lerp(
                alpha,
                self.first.sample_value(context, block_x, block_y, block_z),
                self.second,
            )
        }
    }
}

/// `LerpFunction.Sampler`.
pub struct LerpSampler {
    alpha: SharedSampler,
    first: SharedSampler,
    second: SharedSampler,
}

impl LerpSampler {
    pub fn new(alpha: SharedSampler, first: SharedSampler, second: SharedSampler) -> LerpSampler {
        LerpSampler {
            alpha,
            first,
            second,
        }
    }
}

impl DensitySampler for LerpSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.alpha.sample_volume(context, out, volume);

        let mut first_buffer = context.acquire_buffer(volume);
        self.first.sample_volume(context, &mut first_buffer, volume);

        let mut second_buffer = context.acquire_buffer(volume);
        self.second
            .sample_volume(context, &mut second_buffer, volume);

        for i in 0..out.size() {
            out.set(
                i,
                lerp_element(out.get(i), first_buffer.get(i), second_buffer.get(i)),
            );
        }
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let alpha = self.alpha.sample_value(context, block_x, block_y, block_z);
        if alpha == 0.0 {
            self.first.sample_value(context, block_x, block_y, block_z)
        } else if alpha == 1.0 {
            self.second.sample_value(context, block_x, block_y, block_z)
        } else {
            mth::lerp(
                alpha,
                self.first.sample_value(context, block_x, block_y, block_z),
                self.second.sample_value(context, block_x, block_y, block_z),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::function::PreparedCache;
    use crate::densityfunction::generator::constant::ConstantSampler;
    use crate::densityfunction::generator::constant::tests::{NoCtx, assert_volume_matches_value};

    /// Alpha ramp: `y_clamped_gradient(0, 4, 0, 1)` = `y / 4` clamped to `[0, 1]`.
    fn alpha() -> DensityFunction {
        DensityFunction::y_clamped_gradient(0, 4, 0.0, 1.0)
    }

    fn at(function: &DensityFunction, y: i32) -> f32 {
        function
            .compile_sampler(&NoCtx)
            .sample_value(&SamplerContext::empty_uncached(), 0, y, 0)
    }

    /// Opaque (non-`Constant`) function with a fixed value.
    fn opaque(value: f32) -> DensityFunction {
        DensityFunction::new(DfNode::PreparedCache(PreparedCache {
            id: 0,
            range: Interval::of_exact(value),
            domain_axes: 0,
            caching_sampler: Arc::new(ConstantSampler::new(value)),
        }))
    }

    #[test]
    fn lerps_between_first_and_second() {
        let function = DensityFunction::lerp(alpha(), opaque(10.0), opaque(20.0));
        let values: Vec<f32> = [-5, 0, 1, 2, 4, 9]
            .iter()
            .map(|&y| at(&function, y))
            .collect();
        assert_eq!(values, [10.0, 10.0, 12.5, 15.0, 20.0, 20.0]);
    }

    #[test]
    fn const_first_and_second_agree_with_generic_lerp() {
        let const_first =
            DensityFunction::lerp(alpha(), DensityFunction::constant(10.0), opaque(20.0));
        let const_second =
            DensityFunction::lerp(alpha(), opaque(10.0), DensityFunction::constant(20.0));
        for y in -1..6 {
            let expected = at(
                &DensityFunction::lerp(alpha(), opaque(10.0), opaque(20.0)),
                y,
            );
            assert_eq!(at(&const_first, y), expected);
            assert_eq!(at(&const_second, y), expected);
        }
    }

    #[test]
    fn alpha_ends_select_exactly_without_lerping() {
        // lerp(0, 1, NaN) would be NaN; alpha == 0 / 1 must pick an end untouched.
        let function =
            DensityFunction::lerp(alpha(), opaque(1.0), DensityFunction::constant(f32::NAN));
        assert_eq!(at(&function, 0), 1.0);
        assert!(at(&function, 2).is_nan());
        let reversed =
            DensityFunction::lerp(alpha(), DensityFunction::constant(f32::NAN), opaque(7.0));
        assert!(at(&reversed, 0).is_nan());
        assert_eq!(at(&reversed, 4), 7.0);
    }

    #[test]
    fn volume_path_matches_point_path() {
        for function in [
            DensityFunction::lerp(alpha(), opaque(10.0), opaque(20.0)),
            DensityFunction::lerp(
                alpha(),
                DensityFunction::constant(-3.0),
                DensityFunction::y_clamped_gradient(-8, 8, 0.0, 9.0),
            ),
            DensityFunction::lerp(
                alpha(),
                DensityFunction::y_clamped_gradient(-8, 8, 0.0, 9.0),
                DensityFunction::constant(-3.0),
            ),
        ] {
            assert_volume_matches_value(&*function.compile_sampler(&NoCtx));
        }
    }

    #[test]
    fn range_and_axes() {
        let function = DensityFunction::lerp(
            alpha(),
            DensityFunction::constant(10.0),
            DensityFunction::constant(20.0),
        );
        assert_eq!(function.range(), Interval::of(10.0, 20.0));
        assert_eq!(function.domain_axes(), 2);
    }
}
