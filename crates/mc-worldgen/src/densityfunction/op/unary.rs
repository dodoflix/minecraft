//! `op/UnaryFunction.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, mth, signum};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryType {
    Abs,
    Square,
    Cube,
    Sqrt,
    HalfNegative,
    QuarterNegative,
    Reciprocal,
    Negate,
    Squeeze,
    Log,
    Sign,
}

impl UnaryType {
    pub const ALL: [UnaryType; 11] = [
        Self::Abs,
        Self::Square,
        Self::Cube,
        Self::Sqrt,
        Self::HalfNegative,
        Self::QuarterNegative,
        Self::Reciprocal,
        Self::Negate,
        Self::Squeeze,
        Self::Log,
        Self::Sign,
    ];

    /// "abs", "square", "cube", "sqrt", "half_negative", "quarter_negative", "reciprocal", "negate",
    /// "squeeze", "log", "sign".
    pub fn id(self) -> &'static str {
        match self {
            Self::Abs => "abs",
            Self::Square => "square",
            Self::Cube => "cube",
            Self::Sqrt => "sqrt",
            Self::HalfNegative => "half_negative",
            Self::QuarterNegative => "quarter_negative",
            Self::Reciprocal => "reciprocal",
            Self::Negate => "negate",
            Self::Squeeze => "squeeze",
            Self::Log => "log",
            Self::Sign => "sign",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UnaryFunction {
    pub ty: UnaryType,
    pub input: DensityFunction,
}

impl DfOp for UnaryFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let input = self.input.compile_sampler(ctx);
        match self.ty {
            UnaryType::Abs => Arc::new(AbsSampler::new(input)),
            UnaryType::Square => Arc::new(SquareSampler::new(input)),
            UnaryType::Cube => Arc::new(CubeSampler::new(input)),
            UnaryType::Sqrt => Arc::new(SqrtSampler::new(input)),
            UnaryType::HalfNegative => Arc::new(LeakyReLUSampler::new(input, 0.5)),
            UnaryType::QuarterNegative => Arc::new(LeakyReLUSampler::new(input, 0.25)),
            UnaryType::Reciprocal => Arc::new(ReciprocalSampler::new(input)),
            UnaryType::Negate => Arc::new(NegateSampler::new(input)),
            UnaryType::Squeeze => Arc::new(SqueezeSampler::new(input)),
            UnaryType::Log => Arc::new(LogSampler::new(input)),
            UnaryType::Sign => Arc::new(SignSampler::new(input)),
        }
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
            DensityFunction::new(DfNode::Unary(UnaryFunction { ty: self.ty, input }))
        }
    }
    fn range(&self) -> Interval {
        let input = self.input.range();
        match self.ty {
            UnaryType::Abs => Interval::abs(input),
            UnaryType::Square => Interval::square(input),
            UnaryType::Cube => Interval::map_monotonic(input, mth::cube),
            UnaryType::Sqrt => Interval::pow(input, Interval::of_exact(0.5)),
            UnaryType::HalfNegative => {
                Interval::map_monotonic(input, |value| LeakyReLUSampler::apply(0.5, value))
            }
            UnaryType::QuarterNegative => {
                Interval::map_monotonic(input, |value| LeakyReLUSampler::apply(0.25, value))
            }
            UnaryType::Reciprocal => Interval::reciprocal(input),
            UnaryType::Negate => Interval::sub(Interval::of_exact(0.0), input),
            UnaryType::Squeeze => Interval::map_monotonic(input, SqueezeSampler::apply),
            UnaryType::Log => Interval::log(input),
            UnaryType::Sign => Interval::sign(input),
        }
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes()
    }
}

/// Applies `f` to every element of `out` (the shared body of the in-place unary volume loops).
pub(crate) fn map_buffer(out: &mut DensityBuffer, f: impl Fn(f32) -> f32) {
    for value in out.as_mut_slice() {
        *value = f(*value);
    }
}

/// Defines a unary sampler `$name` computing `$apply(input)` on both the volume and the point path.
macro_rules! unary_sampler {
    ($(#[$doc:meta])* $name:ident, $apply:expr) => {
        $(#[$doc])*
        pub struct $name {
            input: SharedSampler,
        }

        impl $name {
            pub fn new(input: SharedSampler) -> $name {
                $name { input }
            }
        }

        impl DensitySampler for $name {
            fn sample_volume(&self, context: &SamplerContext, out: &mut DensityBuffer, volume: &DensityVolume) {
                self.input.sample_volume(context, out, volume);
                map_buffer(out, $apply);
            }
            fn sample_value(&self, context: &SamplerContext, block_x: i32, block_y: i32, block_z: i32) -> f32 {
                ($apply)(self.input.sample_value(context, block_x, block_y, block_z))
            }
        }
    };
}

unary_sampler!(
    /// `UnaryFunction.AbsSampler`.
    AbsSampler,
    f32::abs
);
unary_sampler!(
    /// `UnaryFunction.CubeSampler`.
    CubeSampler,
    mth::cube
);
unary_sampler!(
    /// `UnaryFunction.LogSampler`: `(float) Math.log(double)`.
    LogSampler,
    |value: f32| (value as f64).ln() as f32
);
unary_sampler!(
    /// `UnaryFunction.NegateSampler`.
    NegateSampler,
    |value: f32| -value
);
unary_sampler!(
    /// `UnaryFunction.ReciprocalSampler`.
    ReciprocalSampler,
    |value: f32| 1.0 / value
);
unary_sampler!(
    /// `UnaryFunction.SignSampler`: `Math.signum`.
    SignSampler,
    signum
);
unary_sampler!(
    /// `UnaryFunction.SqrtSampler`.
    SqrtSampler,
    mth::sqrt
);
unary_sampler!(
    /// `UnaryFunction.SquareSampler`.
    SquareSampler,
    mth::square
);
unary_sampler!(
    /// `UnaryFunction.SqueezeSampler`.
    SqueezeSampler,
    SqueezeSampler::apply
);

impl SqueezeSampler {
    fn apply(input: f32) -> f32 {
        let clamped_input = mth::clamp(input, -1.0, 1.0);
        clamped_input / 2.0 - mth::cube(clamped_input) / 24.0
    }
}

/// `UnaryFunction.LeakyReLUSampler`.
pub struct LeakyReLUSampler {
    input: SharedSampler,
    negative_factor: f32,
}

impl LeakyReLUSampler {
    pub fn new(input: SharedSampler, negative_factor: f32) -> LeakyReLUSampler {
        LeakyReLUSampler {
            input,
            negative_factor,
        }
    }

    fn apply(negative_factor: f32, input: f32) -> f32 {
        if input > 0.0 {
            input
        } else {
            input * negative_factor
        }
    }
}

impl DensitySampler for LeakyReLUSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);
        map_buffer(out, |value| Self::apply(self.negative_factor, value));
    }
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        Self::apply(
            self.negative_factor,
            self.input.sample_value(context, block_x, block_y, block_z),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::generator::constant::tests::{
        NoCtx, assert_volume_matches_value, y_ramp,
    };

    fn point(function: &DensityFunction, y: i32) -> f32 {
        function
            .compile_sampler(&NoCtx)
            .sample_value(&SamplerContext::empty_uncached(), 0, y, 0)
    }

    fn of(value: f32, ty: UnaryType) -> f32 {
        point(&DensityFunction::constant(value).unary(ty), 0)
    }

    #[test]
    fn sign_keeps_negative_zero_and_nan() {
        assert_eq!(of(-0.0, UnaryType::Sign).to_bits(), (-0.0_f32).to_bits());
        assert_eq!(of(0.0, UnaryType::Sign).to_bits(), 0.0_f32.to_bits());
        assert_eq!(of(7.5, UnaryType::Sign), 1.0);
        assert_eq!(of(-0.25, UnaryType::Sign), -1.0);
        assert!(of(f32::NAN, UnaryType::Sign).is_nan());
    }

    #[test]
    fn squeeze_clamps_then_applies_cubic() {
        let at_one = 0.5 - 1.0 / 24.0;
        assert!((of(2.0, UnaryType::Squeeze) - at_one).abs() < 1e-7);
        assert!((of(-9.0, UnaryType::Squeeze) + at_one).abs() < 1e-7);
        assert!((of(0.5, UnaryType::Squeeze) - (0.25 - 0.125 / 24.0)).abs() < 1e-7);
        assert_eq!(of(0.0, UnaryType::Squeeze), 0.0);
    }

    #[test]
    fn leaky_relu_scales_only_non_positive_inputs() {
        assert_eq!(of(-4.0, UnaryType::HalfNegative), -2.0);
        assert_eq!(of(-4.0, UnaryType::QuarterNegative), -1.0);
        assert_eq!(of(4.0, UnaryType::QuarterNegative), 4.0);
        assert_eq!(
            of(-0.0, UnaryType::HalfNegative).to_bits(),
            (-0.0_f32).to_bits()
        );
    }

    #[test]
    fn reciprocal_log_sqrt_edge_values() {
        assert_eq!(of(0.0, UnaryType::Reciprocal), f32::INFINITY);
        assert_eq!(of(-0.0, UnaryType::Reciprocal), f32::NEG_INFINITY);
        assert_eq!(of(4.0, UnaryType::Reciprocal), 0.25);
        assert_eq!(of(1.0, UnaryType::Log), 0.0);
        assert_eq!(of(0.0, UnaryType::Log), f32::NEG_INFINITY);
        assert!(of(-1.0, UnaryType::Log).is_nan());
        assert_eq!(of(2.25, UnaryType::Sqrt), 1.5);
        assert!(of(-1.0, UnaryType::Sqrt).is_nan());
    }

    #[test]
    fn simple_ops() {
        assert_eq!(of(-3.0, UnaryType::Abs), 3.0);
        assert_eq!(of(-3.0, UnaryType::Square), 9.0);
        assert_eq!(of(-3.0, UnaryType::Cube), -27.0);
        assert_eq!(of(2.0, UnaryType::Negate), -2.0);
    }

    #[test]
    fn ranges() {
        let range = |ty: UnaryType, min: f32, max: f32| {
            let input = DensityFunction::y_clamped_gradient(0, 1, min, max);
            input.unary(ty).range()
        };
        assert_eq!(range(UnaryType::Square, -2.0, 3.0), Interval::of(0.0, 9.0));
        assert_eq!(range(UnaryType::Abs, -2.0, 3.0), Interval::of(0.0, 3.0));
        assert_eq!(range(UnaryType::Cube, -2.0, 3.0), Interval::of(-8.0, 27.0));
        assert_eq!(range(UnaryType::Negate, 1.0, 2.0), Interval::of(-2.0, -1.0));
        assert_eq!(
            range(UnaryType::HalfNegative, -4.0, 2.0),
            Interval::of(-2.0, 2.0)
        );
        assert_eq!(
            range(UnaryType::QuarterNegative, -4.0, 2.0),
            Interval::of(-1.0, 2.0)
        );
        assert_eq!(range(UnaryType::Sqrt, 0.0, 4.0), Interval::of(0.0, 2.0));
        assert_eq!(range(UnaryType::Sign, -1.0, 5.0), Interval::of(-1.0, 1.0));
        assert_eq!(
            range(UnaryType::Reciprocal, 2.0, 4.0),
            Interval::of(0.25, 0.5)
        );
        assert_eq!(range(UnaryType::Reciprocal, -1.0, 1.0), Interval::INFINITE);
        let squeezed = range(UnaryType::Squeeze, -3.0, 3.0);
        assert!(
            (squeezed.max() - (0.5 - 1.0 / 24.0)).abs() < 1e-7 && squeezed.min() == -squeezed.max()
        );
    }

    #[test]
    fn volume_path_matches_point_path() {
        for ty in UnaryType::ALL {
            let function = DensityFunction::y_clamped_gradient(-8, 8, -4.0, 4.0).unary(ty);
            assert_volume_matches_value(&*function.compile_sampler(&NoCtx));
        }
        assert_volume_matches_value(&SquareSampler::new(y_ramp()));
    }

    #[test]
    fn rewrite_children_preserves_identity_when_unchanged() {
        let function = DensityFunction::constant(1.0).abs();
        let same = function.rewrite_children(&|f: &DensityFunction| f.clone());
        assert!(same.ptr_eq(&function));
        let changed =
            function.rewrite_children(&|_: &DensityFunction| DensityFunction::constant(2.0));
        assert!(!changed.ptr_eq(&function));
        assert_eq!(changed, DensityFunction::constant(2.0).abs());
    }

    #[test]
    fn ids() {
        assert_eq!(UnaryType::QuarterNegative.id(), "quarter_negative");
        assert_eq!(UnaryType::ALL.len(), 11);
    }
}
