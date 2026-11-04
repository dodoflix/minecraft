//! `RangeChoiceFunction`: `when_in_range` where `min_inclusive <= input < max_exclusive`, else `when_out_of_range`.
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::generator::ConstantFunction;
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::Interval;

#[derive(Clone, Debug)]
pub struct RangeChoiceFunction {
    pub input: DensityFunction,
    pub min_inclusive: f32,
    pub max_exclusive: f32,
    pub when_in_range: DensityFunction,
    pub when_out_of_range: DensityFunction,
}
impl PartialEq for RangeChoiceFunction {
    fn eq(&self, o: &Self) -> bool {
        self.input == o.input
            && self.min_inclusive.to_bits() == o.min_inclusive.to_bits()
            && self.max_exclusive.to_bits() == o.max_exclusive.to_bits()
            && self.when_in_range == o.when_in_range
            && self.when_out_of_range == o.when_out_of_range
    }
}
impl Eq for RangeChoiceFunction {}
impl Hash for RangeChoiceFunction {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.input.hash(s);
        self.min_inclusive.to_bits().hash(s);
        self.max_exclusive.to_bits().hash(s);
        self.when_in_range.hash(s);
        self.when_out_of_range.hash(s);
    }
}

impl DfOp for RangeChoiceFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let input = self.input.compile_sampler(ctx);
        if let (
            DfNode::Constant(ConstantFunction {
                value: in_range, ..
            }),
            DfNode::Constant(ConstantFunction {
                value: out_of_range,
                ..
            }),
        ) = (self.when_in_range.node(), self.when_out_of_range.node())
        {
            return Arc::new(ConstSampler::new(
                input,
                self.min_inclusive,
                self.max_exclusive,
                *in_range,
                *out_of_range,
            ));
        }
        Arc::new(Sampler::new(
            input,
            self.min_inclusive,
            self.max_exclusive,
            self.when_in_range.compile_sampler(ctx),
            self.when_out_of_range.compile_sampler(ctx),
        ))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let input = rule.rewrite(&self.input);
        let when_in_range = rule.rewrite(&self.when_in_range);
        let when_out_of_range = rule.rewrite(&self.when_out_of_range);
        if input.ptr_eq(&self.input)
            && when_in_range.ptr_eq(&self.when_in_range)
            && when_out_of_range.ptr_eq(&self.when_out_of_range)
        {
            this.clone()
        } else {
            DensityFunction::new(DfNode::RangeChoice(RangeChoiceFunction {
                input,
                min_inclusive: self.min_inclusive,
                max_exclusive: self.max_exclusive,
                when_in_range,
                when_out_of_range,
            }))
        }
    }
    fn range(&self) -> Interval {
        Interval::encapsulating(&[self.when_in_range.range(), self.when_out_of_range.range()])
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes()
            | self.when_in_range.domain_axes()
            | self.when_out_of_range.domain_axes()
    }
}

/// `RangeChoiceFunction.ConstSampler`: both branches are constants.
pub struct ConstSampler {
    input: SharedSampler,
    min_inclusive: f32,
    max_exclusive: f32,
    when_in_range: f32,
    when_out_of_range: f32,
}

impl ConstSampler {
    pub fn new(
        input: SharedSampler,
        min_inclusive: f32,
        max_exclusive: f32,
        when_in_range: f32,
        when_out_of_range: f32,
    ) -> ConstSampler {
        ConstSampler {
            input,
            min_inclusive,
            max_exclusive,
            when_in_range,
            when_out_of_range,
        }
    }

    fn choose(&self, input: f32) -> f32 {
        if input >= self.min_inclusive && input < self.max_exclusive {
            self.when_in_range
        } else {
            self.when_out_of_range
        }
    }
}

impl DensitySampler for ConstSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);
        for i in 0..out.size() {
            out.set(i, self.choose(out.get(i)));
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        self.choose(self.input.sample_value(context, block_x, block_y, block_z))
    }
}

/// `RangeChoiceFunction.Sampler`.
pub struct Sampler {
    input: SharedSampler,
    min_inclusive: f32,
    max_exclusive: f32,
    when_in_range: SharedSampler,
    when_out_of_range: SharedSampler,
}

impl Sampler {
    pub fn new(
        input: SharedSampler,
        min_inclusive: f32,
        max_exclusive: f32,
        when_in_range: SharedSampler,
        when_out_of_range: SharedSampler,
    ) -> Sampler {
        Sampler {
            input,
            min_inclusive,
            max_exclusive,
            when_in_range,
            when_out_of_range,
        }
    }
}

impl DensitySampler for Sampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.when_in_range.sample_volume(context, out, volume);
        let mut input_buffer = context.acquire_buffer(volume);
        self.input.sample_volume(context, &mut input_buffer, volume);
        let mut when_out_of_range_buffer = context.acquire_buffer(volume);
        self.when_out_of_range
            .sample_volume(context, &mut when_out_of_range_buffer, volume);
        for i in 0..out.size() {
            let input = input_buffer.get(i);
            if !(input >= self.min_inclusive) || !(input < self.max_exclusive) {
                out.set(i, when_out_of_range_buffer.get(i));
            }
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let input_value = self.input.sample_value(context, block_x, block_y, block_z);
        if input_value >= self.min_inclusive && input_value < self.max_exclusive {
            self.when_in_range
                .sample_value(context, block_x, block_y, block_z)
        } else {
            self.when_out_of_range
                .sample_value(context, block_x, block_y, block_z)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::op::slice::test_support::{NoContext, fn_sampler, opaque};

    /// Input value at block x = 0..=5: below, at min, inside, at max, above, NaN.
    const INPUTS: [f32; 6] = [-0.5, 0.0, 0.5, 1.0, 2.0, f32::NAN];

    fn input() -> SharedSampler {
        fn_sampler(|x, _, _| INPUTS[x as usize])
    }

    #[test]
    fn min_is_inclusive_and_max_is_exclusive() {
        let context = SamplerContext::empty_uncached();
        let volume = DensityVolume::with_unit_step(6, 1, 1, 0, 0, 0);
        let expect = [7.0, 5.0, 5.0, 7.0, 7.0, 7.0];

        let constant = ConstSampler::new(input(), 0.0, 1.0, 5.0, 7.0);
        let mut out = DensityBuffer::create_unpooled(6);
        constant.sample_volume(&context, &mut out, &volume);
        assert_eq!(out.as_slice(), expect);
        assert_eq!(
            [0, 1, 2, 3, 4, 5].map(|x| constant.sample_value(&context, x, 0, 0)),
            expect
        );

        let general = Sampler::new(
            input(),
            0.0,
            1.0,
            fn_sampler(|_, _, _| 5.0),
            fn_sampler(|_, _, _| 7.0),
        );
        let mut out = DensityBuffer::create_unpooled(6);
        general.sample_volume(&context, &mut out, &volume);
        assert_eq!(out.as_slice(), expect);
        assert_eq!(
            [0, 1, 2, 3, 4, 5].map(|x| general.sample_value(&context, x, 0, 0)),
            expect
        );
    }

    #[test]
    fn general_sampler_only_evaluates_the_chosen_branch_for_points() {
        let general = Sampler::new(
            fn_sampler(|_, _, _| 0.5),
            0.0,
            1.0,
            fn_sampler(|x, y, z| (x + y + z) as f32),
            fn_sampler(|_, _, _| panic!("out of range branch sampled")),
        );
        assert_eq!(
            general.sample_value(&SamplerContext::empty_uncached(), 1, 2, 3),
            6.0
        );
    }

    fn range_choice(in_range: DensityFunction, out_of_range: DensityFunction) -> DensityFunction {
        opaque(Interval::INFINITE, 0, input()).range_choice(0.0, 1.0, in_range, out_of_range)
    }

    #[test]
    fn constant_branches_compile_to_distinct_in_and_out_values() {
        // Matches vanilla (verified against the real classes in tests/oracle/OpsOracle.java).
        let context = SamplerContext::empty_uncached();
        let sampler = range_choice(
            DensityFunction::constant(5.0),
            DensityFunction::constant(7.0),
        )
        .compile_sampler(&NoContext);
        assert_eq!(
            [0, 1, 4].map(|x| sampler.sample_value(&context, x, 0, 0)),
            [7.0, 5.0, 7.0]
        );
    }

    #[test]
    fn mixed_branches_compile_to_the_general_sampler() {
        let context = SamplerContext::empty_uncached();
        let branch = opaque(
            Interval::INFINITE,
            1,
            fn_sampler(|x, _, _| 100.0 + x as f32),
        );
        let sampler =
            range_choice(branch, DensityFunction::constant(7.0)).compile_sampler(&NoContext);
        assert_eq!(
            [1, 2, 3].map(|x| sampler.sample_value(&context, x, 0, 0)),
            [101.0, 102.0, 7.0]
        );
    }

    #[test]
    fn range_domain_axes_and_identity() {
        let function = opaque(Interval::of(0.0, 1.0), 1, input()).range_choice(
            0.0,
            1.0,
            opaque(Interval::of(-2.0, 0.5), 2, input()),
            opaque(Interval::of(3.0, 4.0), 4, input()),
        );
        assert_eq!(function.range(), Interval::of(-2.0, 4.0));
        assert_eq!(function.domain_axes(), 7);
        assert!(
            function
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&function)
        );
        let rewritten = function.rewrite_children(&|f: &DensityFunction| f.clone().abs());
        assert!(
            matches!(rewritten.node(), DfNode::RangeChoice(r) if r.min_inclusive == 0.0 && r.max_exclusive == 1.0)
        );
    }
}
