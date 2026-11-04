//! `FindTopSurfaceFunction`: the highest probe height (a multiple of `cell_height`, at most the upper bound)
//! at which the density is positive, searched downwards; uniform along Y.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfNode, DfOp};
use crate::densityfunction::op::slice::YSampler;
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::util::{Interval, jmax, mth};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FindTopSurfaceFunction {
    pub density: DensityFunction,
    pub upper_bound: DensityFunction,
    pub lower_bound: i32,
    pub cell_height: i32,
}

impl DfOp for FindTopSurfaceFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let sampler = Sampler::new(
            self.density.compile_sampler(ctx),
            self.upper_bound.compile_sampler(ctx),
            self.lower_bound,
            self.cell_height,
        );
        Arc::new(YSampler::new(Arc::new(sampler), 0))
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        let density = rule.rewrite(&self.density);
        let upper_bound = rule.rewrite(&self.upper_bound);
        if density.ptr_eq(&self.density) && upper_bound.ptr_eq(&self.upper_bound) {
            this.clone()
        } else {
            DensityFunction::new(DfNode::FindTopSurface(FindTopSurfaceFunction {
                density,
                upper_bound,
                lower_bound: self.lower_bound,
                cell_height: self.cell_height,
            }))
        }
    }
    fn range(&self) -> Interval {
        let lower_bound = self.lower_bound as f32;
        Interval::of(
            lower_bound,
            jmax(lower_bound, self.upper_bound.range().max()),
        )
    }
    fn domain_axes(&self) -> u8 {
        (self.density.domain_axes() | self.upper_bound.domain_axes()) & !2
    }
}

/// `FindTopSurfaceFunction.Sampler`; only samples volumes with `size_y == 1`.
pub struct Sampler {
    density: SharedSampler,
    upper_bound: SharedSampler,
    lower_bound: i32,
    cell_height: i32,
}

impl Sampler {
    pub fn new(
        density: SharedSampler,
        upper_bound: SharedSampler,
        lower_bound: i32,
        cell_height: i32,
    ) -> Sampler {
        Sampler {
            density,
            upper_bound,
            lower_bound,
            cell_height,
        }
    }

    fn find_surface_from(&self, context: &SamplerContext, x: i32, z: i32, upper_bound: f32) -> f32 {
        let top_y = mth::floor((upper_bound / self.cell_height as f32) as f64)
            .wrapping_mul(self.cell_height);
        if top_y <= self.lower_bound {
            return self.lower_bound as f32;
        }
        let mut probe_y = top_y;
        while probe_y >= self.lower_bound {
            if self.density.sample_value(context, x, probe_y, z) > 0.0 {
                return probe_y as f32;
            }
            probe_y = probe_y.wrapping_sub(self.cell_height);
        }
        self.lower_bound as f32
    }
}

impl DensitySampler for Sampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        assert!(
            volume.size_y == 1,
            "Cannot sample with sizeY={}",
            volume.size_y
        );
        self.upper_bound.sample_volume(context, out, volume);
        let mut index = 0;
        for z in 0..volume.size_z {
            let block_z = volume.block_z(z);
            for x in 0..volume.size_x {
                let block_x = volume.block_x(x);
                let upper_bound = out.get(index);
                out.set(
                    index,
                    self.find_surface_from(context, block_x, block_z, upper_bound),
                );
                index += 1;
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
        let upper_bound = self
            .upper_bound
            .sample_value(context, block_x, block_y, block_z);
        self.find_surface_from(context, block_x, block_z, upper_bound)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::densityfunction::op::slice::test_support::{NoContext, fn_sampler, opaque};

    #[test]
    fn probes_downwards_from_the_cell_aligned_upper_bound() {
        let probes = Arc::new(Mutex::new(Vec::new()));
        let log = probes.clone();
        // Positive strictly below y = 40.
        let density = fn_sampler(move |x, y, z| {
            log.lock().unwrap().push((x, y, z));
            40.0 - y as f32
        });
        let sampler = Sampler::new(density, fn_sampler(|_, _, _| 100.0), -64, 8);
        let context = SamplerContext::empty_uncached();
        // floor(100 / 8) * 8 = 96; 96, 88, ..., 40 are not positive, 32 is.
        assert_eq!(sampler.sample_value(&context, 3, 123, -5), 32.0);
        let expected: Vec<_> = (32..=96).rev().step_by(8).map(|y| (3, y, -5)).collect();
        assert_eq!(*probes.lock().unwrap(), expected);
    }

    #[test]
    fn falls_back_to_the_lower_bound() {
        let never = fn_sampler(|_, _, _| -1.0);
        let sampler = Sampler::new(never, fn_sampler(|_, _, _| 20.0), 0, 8);
        let context = SamplerContext::empty_uncached();
        assert_eq!(sampler.sample_value(&context, 0, 0, 0), 0.0);
        // The upper bound itself at or below the lower bound: no probing at all.
        let below = Sampler::new(
            fn_sampler(|_, _, _| panic!("probed")),
            fn_sampler(|_, _, _| -3.0),
            0,
            8,
        );
        assert_eq!(below.sample_value(&context, 0, 0, 0), 0.0);
    }

    #[test]
    fn volume_path_matches_point_path_and_rejects_tall_volumes() {
        let upper = fn_sampler(|x, _, z| 50.0 + (x * 16 + z) as f32);
        let sampler = Sampler::new(fn_sampler(|x, y, _| (x * 4 - y) as f32), upper, -16, 4);
        let context = SamplerContext::empty_uncached();
        let volume = DensityVolume::with_unit_step(3, 1, 2, 0, 0, 0);
        let mut out = DensityBuffer::create_unpooled(volume.size());
        sampler.sample_volume(&context, &mut out, &volume);
        for z in 0..2 {
            for x in 0..3 {
                assert_eq!(
                    out.get(volume.index_unchecked(x, 0, z)),
                    sampler.sample_value(&context, x, 0, z)
                );
            }
        }
        // x = 2: density = 8 - y, upper = 82 -> top 80, first positive probe is 4.
        assert_eq!(out.get(volume.index_unchecked(2, 0, 0)), 4.0);
        let tall = DensityVolume::with_unit_step(1, 2, 1, 0, 0, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sampler.sample_volume(
                &SamplerContext::empty_uncached(),
                &mut DensityBuffer::create_unpooled(2),
                &tall,
            )
        }));
        assert!(result.is_err());
    }

    #[test]
    fn compiled_function_is_uniform_along_y() {
        let density = opaque(Interval::INFINITE, 7, fn_sampler(|_, y, _| 10.0 - y as f32));
        let upper = opaque(Interval::of(-5.0, 20.0), 5, fn_sampler(|_, _, _| 20.0));
        let function = DensityFunction::new(DfNode::FindTopSurface(FindTopSurfaceFunction {
            density,
            upper_bound: upper,
            lower_bound: -64,
            cell_height: 4,
        }));
        assert_eq!(function.range(), Interval::of(-64.0, 20.0));
        assert_eq!(function.domain_axes(), 5);
        let sampler = function.compile_sampler(&NoContext);
        let context = SamplerContext::empty_uncached();
        // Any block y gives the same answer: first positive probe from 20 down by 4 is 8.
        assert_eq!(sampler.sample_value(&context, 0, 77, 0), 8.0);
        let volume = DensityVolume::with_unit_step(1, 3, 1, 0, 60, 0);
        let mut out = DensityBuffer::create_unpooled(3);
        sampler.sample_volume(&context, &mut out, &volume);
        assert_eq!(out.as_slice(), [8.0, 8.0, 8.0]);
    }
}
