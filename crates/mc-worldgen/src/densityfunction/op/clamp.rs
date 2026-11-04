//! `op/ClampFunction.java`.
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::op::unary::map_buffer;
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, mth};

#[derive(Clone, Debug)]
pub struct ClampFunction {
    pub input: DensityFunction,
    pub min: f32,
    pub max: f32,
}
impl PartialEq for ClampFunction {
    fn eq(&self, o: &Self) -> bool {
        self.input == o.input
            && self.min.to_bits() == o.min.to_bits()
            && self.max.to_bits() == o.max.to_bits()
    }
}
impl Eq for ClampFunction {}
impl Hash for ClampFunction {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.input.hash(s);
        self.min.to_bits().hash(s);
        self.max.to_bits().hash(s);
    }
}

impl DfOp for ClampFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        Arc::new(ClampSampler::new(
            self.input.compile_sampler(ctx),
            self.min,
            self.max,
        ))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let input = rule.rewrite(&self.input);
        if input.ptr_eq(&self.input) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Clamp(ClampFunction {
                input,
                min: self.min,
                max: self.max,
            }))
        }
    }
    fn range(&self) -> Interval {
        Interval::clamp(self.input.range(), self.min, self.max)
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes()
    }
}

/// `ClampFunction.Sampler`.
pub struct ClampSampler {
    input: SharedSampler,
    min: f32,
    max: f32,
}

impl ClampSampler {
    pub fn new(input: SharedSampler, min: f32, max: f32) -> ClampSampler {
        ClampSampler { input, min, max }
    }
}

impl DensitySampler for ClampSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);
        map_buffer(out, |value| mth::clamp(value, self.min, self.max));
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        mth::clamp(
            self.input.sample_value(context, block_x, block_y, block_z),
            self.min,
            self.max,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::generator::constant::tests::{NoCtx, assert_volume_matches_value};

    fn ramp() -> DensityFunction {
        DensityFunction::y_clamped_gradient(-8, 8, -4.0, 4.0)
    }

    #[test]
    fn clamps_both_paths() {
        let function = ramp().clamp(-1.0, 1.5);
        let sampler = function.compile_sampler(&NoCtx);
        let ctx = SamplerContext::empty_uncached();
        let values: Vec<f32> = [-8, -2, 0, 3, 8]
            .iter()
            .map(|&y| sampler.sample_value(&ctx, 0, y, 0))
            .collect();
        assert_eq!(values, [-1.0, -1.0, 0.0, 1.5, 1.5]);
        assert_volume_matches_value(&*sampler);
    }

    #[test]
    fn range_is_clamped_input_range() {
        assert_eq!(ramp().clamp(-1.0, 1.5).range(), Interval::of(-1.0, 1.5));
        assert_eq!(ramp().clamp(5.0, 6.0).range(), Interval::of(5.0, 5.0));
        assert_eq!(ramp().clamp(-9.0, 9.0).domain_axes(), 2);
    }

    #[test]
    fn rewrite_children_preserves_identity_when_unchanged() {
        let function = ramp().clamp(0.0, 1.0);
        assert!(
            function
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&function)
        );
        let rewritten =
            function.rewrite_children(&|_: &DensityFunction| DensityFunction::constant(9.0));
        assert_eq!(rewritten, DensityFunction::constant(9.0).clamp(0.0, 1.0));
    }
}
