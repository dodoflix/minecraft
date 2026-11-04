//! `generator/EndIslandFunction.java`.
use std::sync::Arc;

use crate::densityfunction::DfRewriteRule;
use crate::densityfunction::function::{CompileContext, DensityFunction, DfOp};
use crate::densityfunction::sampler::{
    DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::synth::{Noise, SimplexNoise};
use crate::util::{Interval, jmax, mth};

const ISLAND_THRESHOLD: f32 = -0.9;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EndIslandFunction;

/// `EndIslandFunction.getHeightValue`.
fn get_height_value(island_noise: &impl Noise, section_x: i32, section_z: i32) -> f32 {
    let chunk_x = section_x / 2;
    let chunk_z = section_z / 2;
    let sub_section_x = section_x % 2;
    let sub_section_z = section_z % 2;
    let mut doffs = -100.0_f32;

    for xo in -12..=12 {
        for zo in -12..=12 {
            let total_chunk_x = (chunk_x + xo) as i64;
            let total_chunk_z = (chunk_z + zo) as i64;
            if total_chunk_x * total_chunk_x + total_chunk_z * total_chunk_z > 4096
                && island_noise.get2(total_chunk_x as f64, total_chunk_z as f64) < ISLAND_THRESHOLD
            {
                let island_size = ((total_chunk_x as f32).abs() * 3439.0
                    + (total_chunk_z as f32).abs() * 147.0)
                    % 13.0
                    + 9.0;
                let xd = (sub_section_x - xo * 2) as f32;
                let zd = (sub_section_z - zo * 2) as f32;
                let mut new_doffs = 100.0 - mth::sqrt(xd * xd + zd * zd) * island_size;
                new_doffs = mth::clamp(new_doffs, -100.0, 80.0);
                doffs = jmax(doffs, new_doffs);
            }
        }
    }

    doffs
}

impl DfOp for EndIslandFunction {
    fn compile_sampler(&self, ctx: &dyn CompileContext) -> SharedSampler {
        let mut island_random = ctx.create_end_island_random();
        island_random.consume_count(17292);
        Arc::new(EndIslandSampler {
            island_noise: SimplexNoise::new(&mut *island_random, true),
        })
    }
    fn rewrite_children(
        &self,
        this: &DensityFunction,
        _rule: &dyn DfRewriteRule,
    ) -> DensityFunction {
        this.clone()
    }
    fn range(&self) -> Interval {
        Interval::of(-0.84375, 0.5625)
    }
    fn domain_axes(&self) -> u8 {
        5
    }
}

/// `EndIslandFunction.Sampler`.
struct EndIslandSampler {
    island_noise: SimplexNoise,
}

impl DensitySampler for EndIslandSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        for z in 0..volume.size_z {
            let block_z = volume.block_z(z);
            for x in 0..volume.size_x {
                let block_x = volume.block_x(x);
                let value = self.sample_value(context, block_x, 0, block_z);
                out.set_range(
                    volume.index_unchecked(x, 0, z),
                    volume.size_y as usize,
                    value,
                );
            }
        }
    }
    fn sample_value(
        &self,
        _context: &SamplerContext,
        block_x: i32,
        _block_y: i32,
        block_z: i32,
    ) -> f32 {
        (get_height_value(&self.island_noise, block_x / 8, block_z / 8) - 8.0) / 128.0
    }
}
