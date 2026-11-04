//! `synth.SmearedPerlinNoise`: Perlin noise whose y coordinate is "fudged" to smear it vertically
//! (used by `BlendedNoise`).

use mc_core::random::RandomSource;

use super::Noise;
use super::gradient_noise::{CachedCorners, wrap};
use super::perlin_noise::PerlinNoise;
use crate::densityfunction::sampler::{DensityBuffer, DensityVolume};
use crate::util::{Interval, mth};

const SHIFT_UP_EPSILON: f32 = 1.0E-7;

#[derive(Clone)]
pub struct SmearedPerlinNoise {
    perlin: PerlinNoise,
    fudge_y_scale: f64,
}

impl SmearedPerlinNoise {
    pub fn new(random: &mut dyn RandomSource, fudge_y_scale: f64) -> SmearedPerlinNoise {
        SmearedPerlinNoise {
            perlin: PerlinNoise::new(random),
            fudge_y_scale,
        }
    }

    /// `SmearedPerlinNoise.range(double)`.
    pub fn range_for(fudge_y_scale: f64) -> Interval {
        Interval::of_symmetric((fudge_y_scale.abs() + 2.0) as f32)
    }

    fn compute_fudge_y(&self, original_y: f64, relative_y: f64) -> f64 {
        let fudge_limit = if original_y >= 0.0 && original_y < relative_y {
            original_y
        } else {
            relative_y
        };
        mth::floor(fudge_limit / self.fudge_y_scale + SHIFT_UP_EPSILON as f64) as f64
            * self.fudge_y_scale
    }
}

impl Noise for SmearedPerlinNoise {
    fn range(&self) -> Interval {
        Self::range_for(self.fudge_y_scale)
    }

    /// Inherited `PerlinNoise.get(x, y)`, which dispatches to the overridden 3D `get`.
    fn get2(&self, x: f64, y: f64) -> f32 {
        self.get3(wrap(x), 0.0, wrap(y))
    }

    fn get3(&self, x: f64, y: f64, z: f64) -> f32 {
        let noise = &self.perlin.noise;
        let original_y = y;
        let x = wrap(x) + noise.offset_x;
        let y = wrap(y) + noise.offset_y;
        let z = wrap(z) + noise.offset_z;
        let floor_x = mth::floor(x);
        let floor_y = mth::floor(y);
        let floor_z = mth::floor(z);
        let relative_x = (x - floor_x as f64) as f32;
        let relative_y = y - floor_y as f64;
        let relative_z = (z - floor_z as f64) as f32;
        let fudged_relative_y = (relative_y - self.compute_fudge_y(original_y, relative_y)) as f32;
        self.perlin.sample_and_lerp(
            floor_x,
            floor_y,
            floor_z,
            relative_x,
            fudged_relative_y,
            relative_z,
            relative_y as f32,
        )
    }

    fn add_to_volume(
        &self,
        buffer: &mut DensityBuffer,
        volume: &DensityVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        let noise = &self.perlin.noise;
        let mut corners = CachedCorners::default();
        let mut index = 0;
        for index_z in 0..volume.size_z {
            let z = wrap(volume.block_z(index_z) as f64 * xz_scale) + noise.offset_z;
            let floor_z = mth::floor(z);
            let relative_z = (z - floor_z as f64) as f32;
            let alpha_z = mth::smoothstep(relative_z);
            for index_x in 0..volume.size_x {
                let x = wrap(volume.block_x(index_x) as f64 * xz_scale) + noise.offset_x;
                let floor_x = mth::floor(x);
                let relative_x = (x - floor_x as f64) as f32;
                let x0 = noise.permute(floor_x);
                let x1 = noise.permute(floor_x.wrapping_add(1));
                let alpha_x = mth::smoothstep(relative_x);
                let mut last_floor_y = i32::MIN;
                for index_y in 0..volume.size_y {
                    let original_y = volume.block_y(index_y) as f64 * y_scale;
                    let y = wrap(original_y) + noise.offset_y;
                    let floor_y = mth::floor(y);
                    let relative_y = y - floor_y as f64;
                    let alpha_y = mth::smoothstep(relative_y as f32);
                    if last_floor_y != floor_y {
                        corners.load(noise, x0, x1, floor_y, floor_z, relative_x, relative_z);
                        last_floor_y = floor_y;
                    }
                    let fudged_relative_y =
                        (relative_y - self.compute_fudge_y(original_y, relative_y)) as f32;
                    buffer.add_to(
                        index,
                        amplitude * corners.lerp(alpha_x, alpha_y, alpha_z, fudged_relative_y),
                    );
                    index += 1;
                }
            }
        }
    }
}
