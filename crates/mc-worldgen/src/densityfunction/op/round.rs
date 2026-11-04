//! `op/RoundFunction.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::op::unary::map_buffer;
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, round_f32};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoundType {
    Floor,
    Round,
    Ceil,
    Truncate,
}

impl RoundType {
    pub const ALL: [RoundType; 4] = [Self::Floor, Self::Round, Self::Ceil, Self::Truncate];

    /// "floor", "round", "ceil", "truncate".
    pub fn id(self) -> &'static str {
        match self {
            Self::Floor => "floor",
            Self::Round => "round",
            Self::Ceil => "ceil",
            Self::Truncate => "truncate",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RoundFunction {
    pub ty: RoundType,
    pub input: DensityFunction,
    pub multiple: DensityFunction,
}

/// `RoundFunction.roundToInteger`: `Math.round` goes through `int`, so it saturates and maps NaN to 0.
fn round_to_integer(input: f32, ty: RoundType) -> f32 {
    match ty {
        RoundType::Floor => input.floor(),
        RoundType::Round => round_f32(input),
        RoundType::Ceil => input.ceil(),
        RoundType::Truncate => {
            if input > 0.0 {
                input.floor()
            } else {
                input.ceil()
            }
        }
    }
}

impl DfOp for RoundFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let input = self.input.compile_sampler(ctx);
        if let DfNode::Constant(constant) = self.multiple.node()
            && constant.value == 1.0
        {
            return Arc::new(IntegerMultipleSampler::new(self.ty, input));
        }
        Arc::new(RoundSampler::new(
            self.ty,
            input,
            self.multiple.compile_sampler(ctx),
        ))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let input = rule.rewrite(&self.input);
        let multiple = rule.rewrite(&self.multiple);
        if input.ptr_eq(&self.input) && multiple.ptr_eq(&self.multiple) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Round(RoundFunction {
                ty: self.ty,
                input,
                multiple,
            }))
        }
    }
    fn range(&self) -> Interval {
        let multiple_range = self.multiple.range();
        Interval::mul(
            Interval::map_monotonic(Interval::div(self.input.range(), multiple_range), |value| {
                round_to_integer(value, self.ty)
            }),
            multiple_range,
        )
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes() | self.multiple.domain_axes()
    }
}

/// `RoundFunction.IntegerMultipleSampler`: rounding to a multiple of exactly 1.
struct IntegerMultipleSampler {
    ty: RoundType,
    input: SharedSampler,
}

impl IntegerMultipleSampler {
    fn new(ty: RoundType, input: SharedSampler) -> IntegerMultipleSampler {
        IntegerMultipleSampler { ty, input }
    }
}

impl DensitySampler for IntegerMultipleSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);
        map_buffer(out, |value| round_to_integer(value, self.ty));
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        round_to_integer(
            self.input.sample_value(context, block_x, block_y, block_z),
            self.ty,
        )
    }
}

/// `RoundFunction.Sampler`.
struct RoundSampler {
    ty: RoundType,
    input: SharedSampler,
    multiple: SharedSampler,
}

impl RoundSampler {
    fn new(ty: RoundType, input: SharedSampler, multiple: SharedSampler) -> RoundSampler {
        RoundSampler {
            ty,
            input,
            multiple,
        }
    }

    fn apply(&self, input: f32, multiple: f32) -> f32 {
        if multiple == 0.0 {
            input
        } else {
            round_to_integer(input / multiple, self.ty) * multiple
        }
    }
}

impl DensitySampler for RoundSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);

        let mut multiple_buffer = context.acquire_buffer(volume);
        self.multiple
            .sample_volume(context, &mut multiple_buffer, volume);

        for i in 0..out.size() {
            out.set(i, self.apply(out.get(i), multiple_buffer.get(i)));
        }
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let input = self.input.sample_value(context, block_x, block_y, block_z);
        let multiple = self
            .multiple
            .sample_value(context, block_x, block_y, block_z);
        self.apply(input, multiple)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::generator::constant::tests::{NoCtx, assert_volume_matches_value};

    fn constant(value: f32) -> DensityFunction {
        DensityFunction::constant(value)
    }

    fn eval(ty: RoundType, input: f32, multiple: f32) -> f32 {
        constant(input)
            .round_with(ty, constant(multiple))
            .compile_sampler(&NoCtx)
            .sample_value(&SamplerContext::empty_uncached(), 0, 0, 0)
    }

    #[test]
    fn integer_rounding_of_each_type() {
        let cases = [
            (
                RoundType::Floor,
                [2.5, -2.5, 2.0, -0.5],
                [2.0, -3.0, 2.0, -1.0],
            ),
            (
                RoundType::Ceil,
                [2.5, -2.5, 2.0, -0.5],
                [3.0, -2.0, 2.0, -0.0],
            ),
            (
                RoundType::Round,
                [2.5, -2.5, 2.4, -0.5],
                [3.0, -2.0, 2.0, 0.0],
            ),
            (
                RoundType::Truncate,
                [2.7, -2.7, 2.0, -0.5],
                [2.0, -2.0, 2.0, -0.0],
            ),
        ];
        for (ty, inputs, expected) in cases {
            for (input, expected) in inputs.into_iter().zip(expected) {
                let actual = eval(ty, input, 1.0);
                assert_eq!(
                    actual.to_bits(),
                    f32::to_bits(expected),
                    "{ty:?}({input}) = {actual}"
                );
            }
        }
    }

    #[test]
    fn round_saturates_through_int_and_maps_nan_to_zero() {
        assert_eq!(eval(RoundType::Round, 3.0e10, 1.0), i32::MAX as f32);
        assert_eq!(eval(RoundType::Round, -3.0e10, 1.0), i32::MIN as f32);
        assert_eq!(eval(RoundType::Round, f32::NAN, 1.0), 0.0);
        assert_eq!(eval(RoundType::Floor, 3.0e10, 1.0), 3.0e10);
    }

    #[test]
    fn other_multiples() {
        assert_eq!(eval(RoundType::Floor, 1.3, 0.5), 1.0);
        assert_eq!(eval(RoundType::Ceil, 1.3, 0.5), 1.5);
        assert_eq!(eval(RoundType::Round, 7.0, 4.0), 8.0);
        assert_eq!(eval(RoundType::Truncate, -7.0, 4.0), -4.0);
        // negative multiple: -7 / -4 = 1.75, floor 1, * -4
        assert_eq!(eval(RoundType::Floor, -7.0, -4.0), -4.0);
        // multiple == 0 passes the input through
        assert_eq!(eval(RoundType::Floor, 1.3, 0.0), 1.3);
    }

    #[test]
    fn volume_path_matches_point_path() {
        let ramp = DensityFunction::y_clamped_gradient(-8, 8, -3.3, 4.7);
        let step = DensityFunction::y_clamped_gradient(-8, 8, -2.0, 2.0);
        for ty in RoundType::ALL {
            for multiple in [constant(1.0), constant(0.75), step.clone()] {
                let function = ramp.clone().round_with(ty, multiple);
                assert_volume_matches_value(&*function.compile_sampler(&NoCtx));
            }
        }
    }

    #[test]
    fn range_rounds_quotient_and_rescales() {
        let ramp = DensityFunction::y_clamped_gradient(0, 1, -1.3, 5.2);
        assert_eq!(
            ramp.clone()
                .round_with(RoundType::Floor, constant(1.0))
                .range(),
            Interval::of(-2.0, 5.0)
        );
        assert_eq!(
            ramp.clone()
                .round_with(RoundType::Ceil, constant(2.0))
                .range(),
            Interval::of(0.0, 6.0)
        );
        assert_eq!(
            ramp.round_with(RoundType::Round, constant(2.0))
                .domain_axes(),
            2
        );
        assert_eq!(RoundType::Truncate.id(), "truncate");
    }
}
