//! `SliceFunction`: evaluates its input at a fixed coordinate along one axis, making it uniform along that axis.
use std::sync::Arc;

use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp, axes_from};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::densityfunction::{Axis, DfRewriteRule};
use crate::util::Interval;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SliceFunction {
    pub axis: Axis,
    pub coordinate: i32,
    pub input: DensityFunction,
}

impl DfOp for SliceFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        if let DfNode::Slice(inner) = self.input.node()
            && matches!(
                (self.axis, inner.axis),
                (Axis::X, Axis::Z) | (Axis::Z, Axis::X)
            )
        {
            let (x, z) = if self.axis == Axis::X {
                (self.coordinate, inner.coordinate)
            } else {
                (inner.coordinate, self.coordinate)
            };
            return Arc::new(XzSampler::new(inner.input.compile_sampler(ctx), x, z));
        }
        let input = self.input.compile_sampler(ctx);
        match self.axis {
            Axis::X => Arc::new(XSampler::new(input, self.coordinate)),
            Axis::Y => Arc::new(YSampler::new(input, self.coordinate)),
            Axis::Z => Arc::new(ZSampler::new(input, self.coordinate)),
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
            DensityFunction::new(DfNode::Slice(SliceFunction {
                axis: self.axis,
                coordinate: self.coordinate,
                input,
            }))
        }
    }
    fn range(&self) -> Interval {
        self.input.range()
    }
    fn domain_axes(&self) -> u8 {
        self.input.domain_axes() & !axes_from(self.axis)
    }
}

/// `SliceFunction.XSampler`.
pub struct XSampler {
    input: SharedSampler,
    x: i32,
}

impl XSampler {
    pub fn new(input: SharedSampler, x: i32) -> XSampler {
        XSampler { input, x }
    }
}

impl DensitySampler for XSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        if volume.size_x == 1 && volume.min_block_x == self.x {
            self.input.sample_volume(context, out, volume);
            return;
        }
        let input_volume = DensityVolume::new(
            1,
            volume.size_y,
            volume.size_z,
            self.x,
            volume.min_block_y,
            volume.min_block_z,
            volume.step_block_x,
            volume.step_block_y,
            volume.step_block_z,
        );
        let mut input_buffer = context.acquire_buffer(&input_volume);
        self.input
            .sample_volume(context, &mut input_buffer, &input_volume);
        let mut index = 0;
        for z in 0..volume.size_z {
            for _ in 0..volume.size_x {
                for y in 0..volume.size_y {
                    out.set(
                        index,
                        input_buffer.get(input_volume.index_unchecked(0, y, z)),
                    );
                    index += 1;
                }
            }
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        _block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        self.input.sample_value(context, self.x, block_y, block_z)
    }
}

/// `SliceFunction.XzSampler`.
pub struct XzSampler {
    input: SharedSampler,
    x: i32,
    z: i32,
}

impl XzSampler {
    pub fn new(input: SharedSampler, x: i32, z: i32) -> XzSampler {
        XzSampler { input, x, z }
    }
}

impl DensitySampler for XzSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        if volume.size_x == 1
            && volume.size_z == 1
            && volume.min_block_x == self.x
            && volume.min_block_z == self.z
        {
            self.input.sample_volume(context, out, volume);
            return;
        }
        let input_volume = DensityVolume::new(
            1,
            volume.size_y,
            1,
            self.x,
            volume.min_block_y,
            self.z,
            volume.step_block_x,
            volume.step_block_y,
            volume.step_block_z,
        );
        let mut input_buffer = context.acquire_buffer(&input_volume);
        self.input
            .sample_volume(context, &mut input_buffer, &input_volume);
        for y in 0..volume.size_y {
            let input = input_buffer.get(input_volume.index_unchecked(0, y, 0));
            let mut index = volume.index_unchecked(0, y, 0);
            for _ in 0..volume.size_z {
                for _ in 0..volume.size_x {
                    out.set(index, input);
                    index += volume.size_y as usize;
                }
            }
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        _block_x: i32,
        block_y: i32,
        _block_z: i32,
    ) -> f32 {
        self.input.sample_value(context, self.x, block_y, self.z)
    }
}

/// `SliceFunction.YSampler`.
pub struct YSampler {
    input: SharedSampler,
    y: i32,
}

impl YSampler {
    pub fn new(input: SharedSampler, y: i32) -> YSampler {
        YSampler { input, y }
    }
}

impl DensitySampler for YSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        if volume.size_y == 1 && volume.min_block_y == self.y {
            self.input.sample_volume(context, out, volume);
            return;
        }
        let input_volume = DensityVolume::new(
            volume.size_x,
            1,
            volume.size_z,
            volume.min_block_x,
            self.y,
            volume.min_block_z,
            volume.step_block_x,
            volume.step_block_y,
            volume.step_block_z,
        );
        let mut input_buffer = context.acquire_buffer(&input_volume);
        self.input
            .sample_volume(context, &mut input_buffer, &input_volume);
        for z in 0..volume.size_z {
            for x in 0..volume.size_x {
                let input = input_buffer.get(input_volume.index_unchecked(x, 0, z));
                out.set_range(
                    volume.index_unchecked(x, 0, z),
                    volume.size_y as usize,
                    input,
                );
            }
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        _block_y: i32,
        block_z: i32,
    ) -> f32 {
        self.input.sample_value(context, block_x, self.y, block_z)
    }
}

/// `SliceFunction.ZSampler`.
pub struct ZSampler {
    input: SharedSampler,
    z: i32,
}

impl ZSampler {
    pub fn new(input: SharedSampler, z: i32) -> ZSampler {
        ZSampler { input, z }
    }
}

impl DensitySampler for ZSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        if volume.size_z == 1 && volume.min_block_z == self.z {
            self.input.sample_volume(context, out, volume);
            return;
        }
        let input_volume = DensityVolume::new(
            volume.size_x,
            volume.size_y,
            1,
            volume.min_block_x,
            volume.min_block_y,
            self.z,
            volume.step_block_x,
            volume.step_block_y,
            volume.step_block_z,
        );
        let mut input_buffer = context.acquire_buffer(&input_volume);
        self.input
            .sample_volume(context, &mut input_buffer, &input_volume);
        let mut index = 0;
        for _ in 0..volume.size_z {
            for x in 0..volume.size_x {
                for y in 0..volume.size_y {
                    out.set(
                        index,
                        input_buffer.get(input_volume.index_unchecked(x, y, 0)),
                    );
                    index += 1;
                }
            }
        }
    }

    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        _block_z: i32,
    ) -> f32 {
        self.input.sample_value(context, block_x, block_y, self.z)
    }
}

/// Closure-backed fake sampler shared by the op tests.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::densityfunction::NoiseHolder;
    use crate::densityfunction::function::PreparedCache;
    use crate::densityfunction::sampler::sample_volume_naive;
    use crate::synth::Noise;
    use mc_core::ident::Identifier;
    use mc_core::random::RandomSource;

    /// Compile context for trees that contain no noise or random users.
    pub struct NoContext;

    impl CompileContext for NoContext {
        fn create_noise_sampler(&self, _: &NoiseHolder) -> Arc<dyn Noise> {
            unreachable!("no noise in test trees")
        }
        fn create_random(&self, _: &Identifier) -> Box<dyn RandomSource> {
            unreachable!("no randoms in test trees")
        }
        fn create_end_island_random(&self) -> Box<dyn RandomSource> {
            unreachable!("no randoms in test trees")
        }
    }

    /// A leaf function with the given range and domain axes that compiles to `sampler`.
    pub fn opaque(range: Interval, domain_axes: u8, sampler: SharedSampler) -> DensityFunction {
        DensityFunction::new(DfNode::PreparedCache(PreparedCache {
            id: 0,
            range,
            domain_axes,
            caching_sampler: sampler,
        }))
    }

    /// Samples `f(block_x, block_y, block_z)` and records every volume it is asked to fill.
    pub struct FnSampler<F> {
        f: F,
        pub volumes: Mutex<Vec<DensityVolume>>,
    }

    pub fn fn_sampler<F: Fn(i32, i32, i32) -> f32 + Send + Sync>(f: F) -> Arc<FnSampler<F>> {
        Arc::new(FnSampler {
            f,
            volumes: Mutex::new(Vec::new()),
        })
    }

    impl<F: Fn(i32, i32, i32) -> f32 + Send + Sync> DensitySampler for FnSampler<F> {
        fn sample_volume(
            &self,
            context: &SamplerContext,
            out: &mut DensityBuffer,
            volume: &DensityVolume,
        ) {
            self.volumes.lock().expect("not poisoned").push(*volume);
            sample_volume_naive(context, out, volume, self);
        }
        fn sample_value(&self, _context: &SamplerContext, x: i32, y: i32, z: i32) -> f32 {
            (self.f)(x, y, z)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::fn_sampler;
    use super::*;

    /// Injective in (x, y, z) for the small coordinates used here.
    fn code(x: i32, y: i32, z: i32) -> f32 {
        (x * 10000 + y * 100 + z) as f32
    }

    fn sample(sampler: &dyn DensitySampler, volume: &DensityVolume) -> Vec<f32> {
        let context = SamplerContext::empty_uncached();
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&context, &mut out, volume);
        out.as_slice().to_vec()
    }

    /// Expected volume content when the axes in `fixed` are replaced by the given block coordinate.
    fn expected(
        volume: &DensityVolume,
        fx: Option<i32>,
        fy: Option<i32>,
        fz: Option<i32>,
    ) -> Vec<f32> {
        let mut out = vec![0.0; volume.size()];
        for z in 0..volume.size_z {
            for x in 0..volume.size_x {
                for y in 0..volume.size_y {
                    out[volume.index_unchecked(x, y, z)] = code(
                        fx.unwrap_or(volume.block_x(x)),
                        fy.unwrap_or(volume.block_y(y)),
                        fz.unwrap_or(volume.block_z(z)),
                    );
                }
            }
        }
        out
    }

    #[test]
    fn x_sampler_on_non_matching_volume_samples_one_x_layer() {
        let input = fn_sampler(code);
        let sampler = XSampler::new(input.clone(), 9);
        let volume = DensityVolume::new(3, 2, 4, 5, 1, 3, 2, 1, 1);
        assert_eq!(
            sample(&sampler, &volume),
            expected(&volume, Some(9), None, None)
        );
        assert_eq!(
            *input.volumes.lock().unwrap(),
            [DensityVolume::new(1, 2, 4, 9, 1, 3, 2, 1, 1)]
        );
    }

    #[test]
    fn x_sampler_on_matching_volume_passes_it_through() {
        let input = fn_sampler(code);
        let sampler = XSampler::new(input.clone(), 9);
        let volume = DensityVolume::with_unit_step(1, 2, 2, 9, 1, 3);
        assert_eq!(
            sample(&sampler, &volume),
            expected(&volume, None, None, None)
        );
        assert_eq!(*input.volumes.lock().unwrap(), [volume]);
    }

    #[test]
    fn xz_sampler_broadcasts_the_column() {
        let input = fn_sampler(code);
        let sampler = XzSampler::new(input.clone(), 9, -2);
        let volume = DensityVolume::new(2, 3, 2, 5, 1, 3, 1, 4, 1);
        assert_eq!(
            sample(&sampler, &volume),
            expected(&volume, Some(9), None, Some(-2))
        );
        assert_eq!(
            *input.volumes.lock().unwrap(),
            [DensityVolume::new(1, 3, 1, 9, 1, -2, 1, 4, 1)]
        );

        let matching = DensityVolume::with_unit_step(1, 3, 1, 9, 0, -2);
        assert_eq!(
            sample(&sampler, &matching),
            expected(&matching, None, None, None)
        );
        assert_eq!(input.volumes.lock().unwrap()[1], matching);
        assert_eq!(
            sampler.sample_value(&SamplerContext::empty_uncached(), 100, 4, 100),
            code(9, 4, -2)
        );
    }

    #[test]
    fn y_sampler_fills_each_column_with_the_slice_value() {
        let input = fn_sampler(code);
        let sampler = YSampler::new(input.clone(), 7);
        let volume = DensityVolume::with_unit_step(2, 3, 2, 1, 0, 2);
        assert_eq!(
            sample(&sampler, &volume),
            expected(&volume, None, Some(7), None)
        );
        assert_eq!(
            *input.volumes.lock().unwrap(),
            [DensityVolume::with_unit_step(2, 1, 2, 1, 7, 2)]
        );
        assert_eq!(
            sampler.sample_value(&SamplerContext::empty_uncached(), 1, 100, 2),
            code(1, 7, 2)
        );
    }

    #[test]
    fn z_sampler_on_non_matching_volume_samples_one_z_layer() {
        let input = fn_sampler(code);
        let sampler = ZSampler::new(input.clone(), 4);
        let volume = DensityVolume::with_unit_step(2, 2, 3, 1, 0, 0);
        assert_eq!(
            sample(&sampler, &volume),
            expected(&volume, None, None, Some(4))
        );
        assert_eq!(
            *input.volumes.lock().unwrap(),
            [DensityVolume::with_unit_step(2, 2, 1, 1, 0, 4)]
        );
    }

    #[test]
    fn domain_axes_and_range_come_from_the_input() {
        let input = test_support::opaque(Interval::of(-2.0, 3.0), 7, fn_sampler(code));
        let slice = input.clone().slice(Axis::Y, 5);
        assert_eq!(slice.domain_axes(), 5);
        assert_eq!(slice.range(), Interval::of(-2.0, 3.0));
        assert_eq!(slice.clone().slice(Axis::X, 1).domain_axes(), 4);
    }

    #[test]
    fn rewrite_children_preserves_identity_when_unchanged() {
        let slice = DensityFunction::constant(3.0).slice(Axis::X, 5);
        assert!(
            slice
                .rewrite_children(&|f: &DensityFunction| f.clone())
                .ptr_eq(&slice)
        );
        let rewritten =
            slice.rewrite_children(&|_: &DensityFunction| DensityFunction::constant(4.0));
        assert_eq!(rewritten, DensityFunction::constant(4.0).slice(Axis::X, 5));
    }

    #[test]
    fn nested_x_and_z_slices_compile_into_one_xz_sampler() {
        let input = fn_sampler(code);
        let leaf = test_support::opaque(Interval::INFINITE, 7, input.clone());
        let context = SamplerContext::empty_uncached();
        for tree in [
            leaf.clone().slice(Axis::Z, 3).slice(Axis::X, 5),
            leaf.clone().slice(Axis::X, 5).slice(Axis::Z, 3),
        ] {
            let sampler = tree.compile_sampler(&test_support::NoContext);
            assert_eq!(sampler.sample_value(&context, 100, 4, 100), code(5, 4, 3));
            // A merged XzSampler asks the input for a single 1 x n x 1 column.
            let volume = DensityVolume::with_unit_step(2, 2, 2, 0, 0, 0);
            let mut out = DensityBuffer::create_unpooled(volume.size());
            sampler.sample_volume(&context, &mut out, &volume);
            assert_eq!(
                *input.volumes.lock().unwrap().last().unwrap(),
                DensityVolume::with_unit_step(1, 2, 1, 5, 0, 3)
            );
        }
        // Same-axis nesting is not merged: the outer XSampler wraps the inner one, which wins.
        let tree = leaf.slice(Axis::X, 5).slice(Axis::X, 6);
        assert_eq!(
            tree.compile_sampler(&test_support::NoContext)
                .sample_value(&context, 0, 0, 0),
            code(5, 0, 0)
        );
    }
}
