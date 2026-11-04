//! `op/BinaryFunction.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::op::unary::map_buffer;
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, jmax, jmin};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryType {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
}

impl BinaryType {
    pub const ALL: [BinaryType; 6] = [
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Div,
        Self::Min,
        Self::Max,
    ];

    /// "add", "sub", "mul", "div", "min", "max".
    pub fn id(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BinaryFunction {
    pub ty: BinaryType,
    pub left: DensityFunction,
    pub right: DensityFunction,
}

impl BinaryFunction {
    /// `instanceof ConstantFunction(value)`.
    fn constant_value(function: &DensityFunction) -> Option<f32> {
        match function.node() {
            DfNode::Constant(constant) => Some(constant.value),
            _ => None,
        }
    }
}

impl DfOp for BinaryFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let left = self.left.compile_sampler(ctx);
        let right = self.right.compile_sampler(ctx);
        let left_constant = Self::constant_value(&self.left);
        let right_constant = Self::constant_value(&self.right);

        match self.ty {
            BinaryType::Add => match (left_constant, right_constant) {
                (Some(value), _) => Arc::new(ConstAddSampler::new(right, value)),
                (_, Some(value)) => Arc::new(ConstAddSampler::new(left, value)),
                _ => Arc::new(AddSampler::new(left, right)),
            },
            BinaryType::Sub => match (left_constant, right_constant) {
                (Some(value), _) => Arc::new(ConstSubSampler::new(value, right)),
                (_, Some(value)) => Arc::new(ConstAddSampler::new(left, -value)),
                _ => Arc::new(SubSampler::new(left, right)),
            },
            BinaryType::Mul => match (left_constant, right_constant) {
                (Some(value), _) => Arc::new(ConstMulSampler::new(right, value)),
                (_, Some(value)) => Arc::new(ConstMulSampler::new(left, value)),
                _ => Arc::new(MulSampler::new(left, right)),
            },
            BinaryType::Div => match (left_constant, right_constant) {
                (Some(value), _) => Arc::new(ConstDivSampler::new(value, right)),
                (_, Some(value)) => Arc::new(ConstMulSampler::new(left, 1.0 / value)),
                _ => Arc::new(DivSampler::new(left, right)),
            },
            BinaryType::Min => {
                let left_range = self.left.range();
                let right_range = self.right.range();
                if left_range.max() < right_range.min() {
                    left
                } else if right_range.max() < left_range.min() {
                    right
                } else {
                    match (left_constant, right_constant) {
                        (Some(value), _) => Arc::new(ConstMinSampler::new(right, value)),
                        (_, Some(value)) => Arc::new(ConstMinSampler::new(left, value)),
                        _ => Arc::new(MinSampler::new(left, right, right_range.min())),
                    }
                }
            }
            BinaryType::Max => {
                let left_range = self.left.range();
                let right_range = self.right.range();
                if left_range.min() > right_range.max() {
                    left
                } else if right_range.min() > left_range.max() {
                    right
                } else {
                    match (left_constant, right_constant) {
                        (Some(value), _) => Arc::new(ConstMaxSampler::new(right, value)),
                        (_, Some(value)) => Arc::new(ConstMaxSampler::new(left, value)),
                        _ => Arc::new(MaxSampler::new(left, right, right_range.max())),
                    }
                }
            }
        }
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let left = rule.rewrite(&self.left);
        let right = rule.rewrite(&self.right);
        if left.ptr_eq(&self.left) && right.ptr_eq(&self.right) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::Binary(BinaryFunction {
                ty: self.ty,
                left,
                right,
            }))
        }
    }
    fn range(&self) -> Interval {
        let left = self.left.range();
        let right = self.right.range();
        match self.ty {
            BinaryType::Add => Interval::add(left, right),
            BinaryType::Sub => Interval::sub(left, right),
            BinaryType::Mul => Interval::mul(left, right),
            BinaryType::Div => Interval::div(left, right),
            BinaryType::Min => Interval::min_of(left, right),
            BinaryType::Max => Interval::max_of(left, right),
        }
    }
    fn domain_axes(&self) -> u8 {
        self.left.domain_axes() | self.right.domain_axes()
    }
}

/// Samples `left` into `out`, `right` into a pooled buffer, then `combine(out[i], right[i])` into `out[i]`.
fn sample_volume_combined(
    left: &dyn DensitySampler,
    right: &dyn DensitySampler,
    context: &SamplerContext,
    out: &mut DensityBuffer,
    volume: &DensityVolume,
    combine: impl Fn(f32, f32) -> f32,
) {
    left.sample_volume(context, out, volume);

    let mut right_buffer = context.acquire_buffer(volume);
    right.sample_volume(context, &mut right_buffer, volume);

    for i in 0..out.size() {
        out.set(i, combine(out.get(i), right_buffer.get(i)));
    }
}

/// Defines a sampler combining two samplers; `$volume` merges the buffered values, `$value` is the point
/// path (given the two samplers so it can short-circuit like vanilla).
macro_rules! binary_sampler {
    (
        $(#[$doc:meta])* $name:ident { $($extra:ident: $extra_ty:ty),* },
        volume: $volume:expr,
        value: |$self_:ident, $ctx:ident, $x:ident, $y:ident, $z:ident| $value:expr
    ) => {
        $(#[$doc])*
        pub struct $name {
            left: SharedSampler,
            right: SharedSampler,
            $($extra: $extra_ty),*
        }

        impl $name {
            pub fn new(left: SharedSampler, right: SharedSampler $(, $extra: $extra_ty)*) -> $name {
                $name { left, right $(, $extra)* }
            }
        }

        impl DensitySampler for $name {
            fn sample_volume(&self, context: &SamplerContext, out: &mut DensityBuffer, volume: &DensityVolume) {
                sample_volume_combined(&*self.left, &*self.right, context, out, volume, $volume);
            }
            fn sample_value(&self, $ctx: &SamplerContext, $x: i32, $y: i32, $z: i32) -> f32 {
                let $self_ = self;
                $value
            }
        }
    };
}

binary_sampler!(
    /// `BinaryFunction.AddSampler`.
    AddSampler {},
    volume: |left, right| left + right,
    value: |this, ctx, x, y, z| this.left.sample_value(ctx, x, y, z) + this.right.sample_value(ctx, x, y, z)
);

binary_sampler!(
    /// `BinaryFunction.SubSampler`.
    SubSampler {},
    volume: |left, right| left + -right,
    value: |this, ctx, x, y, z| this.left.sample_value(ctx, x, y, z) - this.right.sample_value(ctx, x, y, z)
);

binary_sampler!(
    /// `BinaryFunction.MulSampler`: a zero left operand short-circuits on the point path (so `0 * inf` is 0).
    MulSampler {},
    volume: |left, right| left * right,
    value: |this, ctx, x, y, z| {
        let left = this.left.sample_value(ctx, x, y, z);
        if left == 0.0 { 0.0 } else { left * this.right.sample_value(ctx, x, y, z) }
    }
);

binary_sampler!(
    /// `BinaryFunction.DivSampler`: a zero left operand short-circuits on the point path.
    DivSampler {},
    volume: |left, right| left / right,
    value: |this, ctx, x, y, z| {
        let left = this.left.sample_value(ctx, x, y, z);
        if left == 0.0 { 0.0 } else { left / this.right.sample_value(ctx, x, y, z) }
    }
);

binary_sampler!(
    /// `BinaryFunction.MaxSampler`: skips `right` on the point path when `left` reaches `right`'s maximum.
    MaxSampler { right_max_value: f32 },
    volume: |left, right| if right > left { right } else { left },
    value: |this, ctx, x, y, z| {
        let left = this.left.sample_value(ctx, x, y, z);
        if left >= this.right_max_value { left } else { jmax(left, this.right.sample_value(ctx, x, y, z)) }
    }
);

binary_sampler!(
    /// `BinaryFunction.MinSampler`: skips `right` on the point path when `left` reaches `right`'s minimum.
    MinSampler { right_min_value: f32 },
    volume: |left, right| if right < left { right } else { left },
    value: |this, ctx, x, y, z| {
        let left = this.left.sample_value(ctx, x, y, z);
        if left <= this.right_min_value { left } else { jmin(left, this.right.sample_value(ctx, x, y, z)) }
    }
);

/// Defines `$name(left sampler, right constant)`; `$volume` and `$value` combine the sampled `$v` with the
/// constant `$c` on the volume and point path.
macro_rules! const_right_sampler {
    (
        $(#[$doc:meta])* $name:ident,
        |$v:ident, $c:ident| { volume: $volume:expr, value: $value:expr }
    ) => {
        $(#[$doc])*
        pub struct $name {
            left: SharedSampler,
            right: f32,
        }

        impl $name {
            pub fn new(left: SharedSampler, right: f32) -> $name {
                $name { left, right }
            }
        }

        impl DensitySampler for $name {
            fn sample_volume(&self, context: &SamplerContext, out: &mut DensityBuffer, volume: &DensityVolume) {
                self.left.sample_volume(context, out, volume);
                let $c = self.right;
                map_buffer(out, |$v: f32| $volume);
            }
            fn sample_value(&self, context: &SamplerContext, block_x: i32, block_y: i32, block_z: i32) -> f32 {
                let $v = self.left.sample_value(context, block_x, block_y, block_z);
                let $c = self.right;
                $value
            }
        }
    };
}

/// Defines `$name(left constant, right sampler)`, like [`const_right_sampler`] with the operands swapped.
macro_rules! const_left_sampler {
    (
        $(#[$doc:meta])* $name:ident,
        |$c:ident, $v:ident| $apply:expr
    ) => {
        $(#[$doc])*
        pub struct $name {
            left: f32,
            right: SharedSampler,
        }

        impl $name {
            pub fn new(left: f32, right: SharedSampler) -> $name {
                $name { left, right }
            }
        }

        impl DensitySampler for $name {
            fn sample_volume(&self, context: &SamplerContext, out: &mut DensityBuffer, volume: &DensityVolume) {
                self.right.sample_volume(context, out, volume);
                let $c = self.left;
                map_buffer(out, |$v: f32| $apply);
            }
            fn sample_value(&self, context: &SamplerContext, block_x: i32, block_y: i32, block_z: i32) -> f32 {
                let $v = self.right.sample_value(context, block_x, block_y, block_z);
                let $c = self.left;
                $apply
            }
        }
    };
}

const_right_sampler!(
    /// `BinaryFunction.ConstAddSampler`: `left + right`.
    ConstAddSampler,
    |v, c| { volume: v + c, value: v + c }
);

const_right_sampler!(
    /// `BinaryFunction.ConstMulSampler`: `left * right`.
    ConstMulSampler,
    |v, c| { volume: v * c, value: v * c }
);

const_right_sampler!(
    /// `BinaryFunction.ConstMaxSampler`: `max(left, right)`; the volume path keeps `left` when `right` is NaN.
    ConstMaxSampler,
    |v, c| { volume: if c > v { c } else { v }, value: jmax(v, c) }
);

const_right_sampler!(
    /// `BinaryFunction.ConstMinSampler`: `min(left, right)`; the volume path keeps `left` when `right` is NaN.
    ConstMinSampler,
    |v, c| { volume: if c < v { c } else { v }, value: jmin(v, c) }
);

const_left_sampler!(
    /// `BinaryFunction.ConstSubSampler`: `left - right`.
    ConstSubSampler,
    |c, v| c - v
);

const_left_sampler!(
    /// `BinaryFunction.ConstDivSampler`: `left / right` (no zero short-circuit, unlike `DivSampler`).
    ConstDivSampler,
    |c, v| c / v
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::function::PreparedCache;
    use crate::densityfunction::generator::constant::ConstantSampler;
    use crate::densityfunction::generator::constant::tests::{NoCtx, assert_volume_matches_value};

    /// `y_clamped_gradient(-8, 8, -4, 4)`: value `y / 2` for `y` in `[-8, 8]`, range `[-4, 4]`.
    fn ramp() -> DensityFunction {
        DensityFunction::y_clamped_gradient(-8, 8, -4.0, 4.0)
    }

    /// A function that declares `range` but always yields `value` (to observe range-based pruning).
    fn declared(range: Interval, value: f32) -> DensityFunction {
        DensityFunction::new(DfNode::PreparedCache(PreparedCache {
            id: 0,
            range,
            domain_axes: 7,
            caching_sampler: Arc::new(ConstantSampler::new(value)),
        }))
    }

    fn at(function: &DensityFunction, y: i32) -> f32 {
        function
            .compile_sampler(&NoCtx)
            .sample_value(&SamplerContext::empty_uncached(), 0, y, 0)
    }

    #[test]
    fn mul_short_circuits_zero_left_only_on_point_path() {
        let infinity = DensityFunction::constant(0.0).reciprocal();
        let product = ramp().mul(infinity.clone());
        // ramp(0) == 0: MulSampler returns 0 instead of 0 * inf = NaN.
        assert_eq!(at(&product, 0), 0.0);
        assert_eq!(at(&product, 2), f32::INFINITY);
        let sampler = product.compile_sampler(&NoCtx);
        let mut out = DensityBuffer::create_unpooled(1);
        sampler.sample_volume(
            &SamplerContext::empty_uncached(),
            &mut out,
            &DensityVolume::with_unit_step(1, 1, 1, 0, 0, 0),
        );
        assert!(out.get(0).is_nan());
        // A constant factor selects ConstMulSampler, which has no short circuit.
        assert!(at(&DensityFunction::constant(0.0).mul(infinity), 0).is_nan());
    }

    #[test]
    fn div_short_circuits_zero_left_and_const_selection() {
        let quotient = ramp().div(ramp());
        assert_eq!(at(&quotient, 0), 0.0);
        assert_eq!(at(&quotient, 6), 1.0);
        // const right: multiplied by the reciprocal; 0 * (1 / 0) = NaN.
        assert!(at(&ramp().div_f(0.0), 0).is_nan());
        assert_eq!(at(&ramp().div_f(4.0), 4), 0.5);
        // const left: ConstDivSampler.
        assert_eq!(at(&DensityFunction::constant(6.0).div(ramp()), 4), 3.0);
        assert_eq!(
            at(&DensityFunction::constant(6.0).div(ramp()), 0),
            f32::INFINITY
        );
    }

    #[test]
    fn add_sub_const_forms() {
        assert_eq!(at(&DensityFunction::constant(10.0).sub(ramp()), 4), 8.0);
        assert_eq!(at(&ramp().sub_f(1.5), 4), 0.5);
        assert_eq!(at(&DensityFunction::constant(1.0).add(ramp()), 4), 3.0);
        assert_eq!(at(&ramp().add(ramp()), -6), -6.0);
        assert_eq!(
            at(&ramp().sub(DensityFunction::constant(0.25).mul(ramp())), 8),
            3.0
        );
    }

    #[test]
    fn min_max_prune_when_ranges_do_not_intersect() {
        let low = || declared(Interval::of(0.0, 1.0), 7.0);
        let high = || declared(Interval::of(5.0, 6.0), -3.0);
        // min: low.max < high.min -> left; high.max < ... -> right
        assert_eq!(at(&low().min(high()), 0), 7.0);
        assert_eq!(at(&high().min(low()), 0), 7.0);
        // max: high.min > low.max -> the high one
        assert_eq!(at(&low().max(high()), 0), -3.0);
        assert_eq!(at(&high().max(low()), 0), -3.0);
        // overlapping ranges are not pruned
        assert_eq!(
            at(&low().min(declared(Interval::of(0.5, 2.0), 0.25)), 0),
            0.25
        );
    }

    #[test]
    fn min_max_point_path_skips_right_when_left_is_beyond_its_range() {
        // `right` lies about its range [0, 1]: it really yields -100.
        let right = declared(Interval::of(0.0, 1.0), -100.0);
        let min = ramp().min(right.clone());
        assert_eq!(at(&min, -6), -3.0); // ramp <= right.min: right never sampled
        assert_eq!(at(&min, 4), -100.0);
        let max = ramp().max(declared(Interval::of(-1.0, 0.0), 100.0));
        assert_eq!(at(&max, 6), 3.0); // ramp >= right.max
        assert_eq!(at(&max, -4), 100.0);
    }

    #[test]
    fn const_min_max() {
        let min = DensityFunction::constant(3.0).min(ramp());
        assert_eq!(at(&min, 8), 3.0);
        assert_eq!(at(&min, 2), 1.0);
        let max = ramp().max(DensityFunction::constant(1.0));
        assert_eq!(at(&max, 8), 4.0);
        assert_eq!(at(&max, -8), 1.0);
    }

    #[test]
    fn volume_path_matches_point_path() {
        let other = DensityFunction::y_clamped_gradient(0, 4, 1.0, 5.0);
        for ty in BinaryType::ALL {
            for function in [
                DensityFunction::binary(ty, ramp(), other.clone()),
                DensityFunction::binary(ty, ramp(), DensityFunction::constant(1.5)),
                DensityFunction::binary(ty, DensityFunction::constant(1.5), other.clone()),
            ] {
                assert_volume_matches_value(&*function.compile_sampler(&NoCtx));
            }
        }
    }

    #[test]
    fn ranges_and_axes() {
        let range = |ty| {
            DensityFunction::binary(
                ty,
                declared(Interval::of(-2.0, 3.0), 0.0),
                declared(Interval::of(1.0, 4.0), 0.0),
            )
            .range()
        };
        assert_eq!(range(BinaryType::Add), Interval::of(-1.0, 7.0));
        assert_eq!(range(BinaryType::Sub), Interval::of(-6.0, 2.0));
        assert_eq!(range(BinaryType::Mul), Interval::of(-8.0, 12.0));
        assert_eq!(range(BinaryType::Min), Interval::of(-2.0, 3.0));
        assert_eq!(range(BinaryType::Max), Interval::of(1.0, 4.0));
        assert_eq!(
            BinaryFunction {
                ty: BinaryType::Add,
                left: ramp(),
                right: DensityFunction::constant(1.0)
            }
            .domain_axes(),
            2
        );
        assert_eq!(BinaryType::Div.id(), "div");
    }

    #[test]
    fn rewrite_children_preserves_identity_when_unchanged() {
        let function = ramp().add(DensityFunction::constant(1.0));
        assert!(
            function
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&function)
        );
        let swapped = function.rewrite_children(&|f: &DensityFunction| {
            if f.domain_axes() == 0 {
                DensityFunction::constant(2.0)
            } else {
                f.clone()
            }
        });
        assert_eq!(swapped, ramp().add(DensityFunction::constant(2.0)));
    }
}
