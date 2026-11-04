//! `IntervalSelectFunction`: picks one of N functions by which threshold interval the input falls into.
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::Interval;

#[derive(Clone, Debug)]
pub struct IntervalSelectFunction {
    pub input: DensityFunction,
    pub thresholds: Vec<f32>,
    pub functions: Vec<DensityFunction>,
}
impl PartialEq for IntervalSelectFunction {
    fn eq(&self, o: &Self) -> bool {
        self.input == o.input
            && self
                .thresholds
                .iter()
                .map(|f| f.to_bits())
                .eq(o.thresholds.iter().map(|f| f.to_bits()))
            && self.functions == o.functions
    }
}
impl Eq for IntervalSelectFunction {}
impl Hash for IntervalSelectFunction {
    fn hash<H: Hasher>(&self, s: &mut H) {
        self.input.hash(s);
        self.thresholds.iter().for_each(|f| f.to_bits().hash(s));
        self.functions.hash(s);
    }
}

impl DfOp for IntervalSelectFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let input = self.input.compile_sampler(ctx);
        if let [threshold] = self.thresholds[..] {
            let if_below = self.functions[0].compile_sampler(ctx);
            let if_above = self.functions[self.functions.len() - 1].compile_sampler(ctx);
            Arc::new(SingleThresholdSampler::new(
                input, threshold, if_below, if_above,
            ))
        } else {
            let samplers = self
                .functions
                .iter()
                .map(|function| function.compile_sampler(ctx))
                .collect();
            Arc::new(Sampler::new(input, self.thresholds.clone(), samplers))
        }
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let input = rule.rewrite(&self.input);
        let functions: Vec<DensityFunction> = self
            .functions
            .iter()
            .map(|function| rule.rewrite(function))
            .collect();
        let functions_changed = functions
            .iter()
            .zip(&self.functions)
            .any(|(new, old)| !new.ptr_eq(old));
        if input.ptr_eq(&self.input) && !functions_changed {
            this.clone()
        } else {
            DensityFunction::new(DfNode::IntervalSelect(IntervalSelectFunction {
                input,
                thresholds: self.thresholds.clone(),
                functions,
            }))
        }
    }
    fn range(&self) -> Interval {
        let ranges: Vec<Interval> = self.functions.iter().map(DensityFunction::range).collect();
        Interval::encapsulating(&ranges)
    }
    fn domain_axes(&self) -> u8 {
        self.functions
            .iter()
            .fold(self.input.domain_axes(), |axes, function| {
                axes | function.domain_axes()
            })
    }
}

/// `IntervalSelectFunction.Sampler`.
pub struct Sampler {
    input: SharedSampler,
    thresholds: Vec<f32>,
    samplers: Vec<SharedSampler>,
}

impl Sampler {
    pub fn new(
        input: SharedSampler,
        thresholds: Vec<f32>,
        samplers: Vec<SharedSampler>,
    ) -> Sampler {
        Sampler {
            input,
            thresholds,
            samplers,
        }
    }

    /// Index of the first threshold strictly above `input`, or the last sampler.
    fn select_sampler_index(&self, input: f32) -> usize {
        self.thresholds
            .iter()
            .position(|&threshold| input < threshold)
            .unwrap_or(self.samplers.len() - 1)
    }
}

impl DensitySampler for Sampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);
        let buffers: Vec<_> = self
            .samplers
            .iter()
            .map(|sampler| {
                let mut buffer = context.acquire_buffer(volume);
                sampler.sample_volume(context, &mut buffer, volume);
                buffer
            })
            .collect();
        for i in 0..out.size() {
            let sampler_index = self.select_sampler_index(out.get(i));
            out.set(i, buffers[sampler_index].get(i));
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
        self.samplers[self.select_sampler_index(input)]
            .sample_value(context, block_x, block_y, block_z)
    }
}

/// `IntervalSelectFunction.SingleThresholdSampler`.
pub struct SingleThresholdSampler {
    input: SharedSampler,
    threshold: f32,
    if_below: SharedSampler,
    if_above: SharedSampler,
}

impl SingleThresholdSampler {
    pub fn new(
        input: SharedSampler,
        threshold: f32,
        if_below: SharedSampler,
        if_above: SharedSampler,
    ) -> SingleThresholdSampler {
        SingleThresholdSampler {
            input,
            threshold,
            if_below,
            if_above,
        }
    }
}

impl DensitySampler for SingleThresholdSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.input.sample_volume(context, out, volume);
        let mut if_below_buffer = context.acquire_buffer(volume);
        self.if_below
            .sample_volume(context, &mut if_below_buffer, volume);
        let mut if_above_buffer = context.acquire_buffer(volume);
        self.if_above
            .sample_volume(context, &mut if_above_buffer, volume);
        for i in 0..out.size() {
            let input = out.get(i);
            out.set(
                i,
                if input < self.threshold {
                    if_below_buffer.get(i)
                } else {
                    if_above_buffer.get(i)
                },
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
        let input = self.input.sample_value(context, block_x, block_y, block_z);
        if input < self.threshold {
            self.if_below
                .sample_value(context, block_x, block_y, block_z)
        } else {
            self.if_above
                .sample_value(context, block_x, block_y, block_z)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::densityfunction::op::slice::test_support::{NoContext, fn_sampler, opaque};

    fn constants(values: &[f32]) -> Vec<SharedSampler> {
        values
            .iter()
            .map(|&v| fn_sampler(move |_, _, _| v) as SharedSampler)
            .collect()
    }

    #[test]
    fn selects_first_threshold_strictly_above_input() {
        let sampler = Sampler::new(
            fn_sampler(|x, _, _| x as f32 * 0.5),
            vec![0.0, 1.0, 1.0, 3.0],
            constants(&[10., 11., 12., 13., 14.]),
        );
        let context = SamplerContext::empty_uncached();
        // Input is x / 2: -1 < 0.0 -> 10; ties go to the next interval (0.0 -> 11); equal thresholds
        // make the interval between them unreachable (1.0 -> 13); 3.0 and above -> last.
        let at = |x| sampler.sample_value(&context, x, 0, 0);
        assert_eq!(
            [at(-2), at(0), at(1), at(2), at(3), at(5), at(6), at(7)],
            [10., 11., 11., 13., 13., 13., 14., 14.]
        );
    }

    #[test]
    fn nan_input_selects_the_last_function() {
        let sampler = Sampler::new(
            fn_sampler(|_, _, _| f32::NAN),
            vec![0.0, 1.0],
            constants(&[1., 2., 3.]),
        );
        assert_eq!(
            sampler.sample_value(&SamplerContext::empty_uncached(), 0, 0, 0),
            3.0
        );
    }

    #[test]
    fn volume_path_agrees_with_point_path() {
        let thresholds = vec![-1.0, 0.0];
        let functions: Vec<SharedSampler> = vec![
            fn_sampler(|x, y, z| (x + y + z) as f32),
            fn_sampler(|x, _, _| 100.0 + x as f32),
            fn_sampler(|_, y, _| -y as f32),
        ];
        let sampler = Sampler::new(fn_sampler(|x, y, _| (x - y) as f32), thresholds, functions);
        let context = SamplerContext::empty_uncached();
        let volume = DensityVolume::with_unit_step(5, 3, 2, -3, 0, 4);
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&context, &mut out, &volume);
        let mut selected = [0; 3];
        for z in 0..2 {
            for x in 0..5 {
                for y in 0..3 {
                    let (bx, by, bz) = (volume.block_x(x), volume.block_y(y), volume.block_z(z));
                    assert_eq!(
                        out.get(volume.index_unchecked(x, y, z)),
                        sampler.sample_value(&context, bx, by, bz)
                    );
                    selected[sampler.select_sampler_index((bx - by) as f32)] += 1;
                }
            }
        }
        assert!(
            selected.iter().all(|&n| n > 0),
            "every branch exercised: {selected:?}"
        );
    }

    #[test]
    fn single_threshold_sampler_picks_above_on_ties() {
        let sampler = SingleThresholdSampler::new(
            fn_sampler(|x, _, _| x as f32),
            2.0,
            fn_sampler(|_, _, _| -1.0),
            fn_sampler(|_, _, _| 1.0),
        );
        let context = SamplerContext::empty_uncached();
        let volume = DensityVolume::with_unit_step(4, 1, 1, 0, 0, 0);
        let mut out = DensityBuffer::create_unpooled(4);
        sampler.sample_volume(&context, &mut out, &volume);
        assert_eq!(out.as_slice(), [-1.0, -1.0, 1.0, 1.0]);
        assert_eq!(
            [1, 2].map(|x| sampler.sample_value(&context, x, 0, 0)),
            [-1.0, 1.0]
        );
    }

    fn select(thresholds: &[f32], functions: Vec<DensityFunction>) -> DensityFunction {
        let input = opaque(Interval::of(-1.0, 1.0), 1, fn_sampler(|x, _, _| x as f32));
        DensityFunction::new(DfNode::IntervalSelect(IntervalSelectFunction {
            input,
            thresholds: thresholds.to_vec(),
            functions,
        }))
    }

    #[test]
    fn range_and_domain_axes_cover_all_branches() {
        let leaf = |lo, hi, axes| opaque(Interval::of(lo, hi), axes, fn_sampler(|_, _, _| 0.0));
        let function = select(
            &[0.0, 1.0],
            vec![leaf(-3.0, -1.0, 2), leaf(0.0, 4.0, 0), leaf(2.0, 3.0, 4)],
        );
        assert_eq!(function.range(), Interval::of(-3.0, 4.0));
        assert_eq!(function.domain_axes(), 7);
    }

    #[test]
    fn compile_uses_single_threshold_sampler_for_two_functions() {
        let leaf = |v: f32| opaque(Interval::of_exact(v), 0, fn_sampler(move |_, _, _| v));
        let context = SamplerContext::empty_uncached();
        let two = select(&[2.0], vec![leaf(5.0), leaf(6.0)]).compile_sampler(&NoContext);
        assert_eq!(
            [1, 2].map(|x| two.sample_value(&context, x, 0, 0)),
            [5.0, 6.0]
        );
        let three =
            select(&[1.0, 2.0], vec![leaf(5.0), leaf(6.0), leaf(7.0)]).compile_sampler(&NoContext);
        assert_eq!(
            [0, 1, 2, 3].map(|x| three.sample_value(&context, x, 0, 0)),
            [5.0, 6.0, 7.0, 7.0]
        );
    }

    #[test]
    fn rewrite_children_preserves_identity_when_unchanged() {
        let function = select(
            &[0.5],
            vec![
                DensityFunction::constant(1.0),
                DensityFunction::constant(2.0),
            ],
        );
        assert!(
            function
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&function)
        );
        let first = function.clone();
        let rewritten = function.rewrite_children(&|f: &DensityFunction| match f.node() {
            DfNode::Constant(c) if c.value == 2.0 => DensityFunction::constant(3.0),
            _ => f.clone(),
        });
        assert!(!rewritten.ptr_eq(&first));
        assert!(matches!(rewritten.node(), DfNode::IntervalSelect(s)
            if s.thresholds == [0.5] && s.functions == [DensityFunction::constant(1.0), DensityFunction::constant(3.0)]));
    }
}
