//! `synth.PerlinNoise`.

use mc_core::random::RandomSource;

use super::Noise;
use super::gradient_noise::{CachedCorners, GradientNoise, grad_dot, wrap};
use crate::densityfunction::sampler::{DensityBuffer, DensityVolume};
use crate::util::{Interval, mth};

#[derive(Clone)]
pub struct PerlinNoise {
    pub(super) noise: GradientNoise,
}

impl PerlinNoise {
    pub const STANDARD_DEVIATION: f64 = 0.2702247831245211;

    pub fn new(random: &mut dyn RandomSource) -> PerlinNoise {
        PerlinNoise {
            noise: GradientNoise::new(random, 256.0),
        }
    }

    /// `noiseWithDerivative`: returns the value and adds the gradient to `derivative_out`.
    pub fn noise_with_derivative(
        &self,
        x: f64,
        y: f64,
        z: f64,
        derivative_out: &mut [f32; 3],
    ) -> f32 {
        let x = wrap(x) + self.noise.offset_x;
        let y = wrap(y) + self.noise.offset_y;
        let z = wrap(z) + self.noise.offset_z;
        let floor_x = mth::floor(x);
        let floor_y = mth::floor(y);
        let floor_z = mth::floor(z);
        let relative_x = (x - floor_x as f64) as f32;
        let relative_y = (y - floor_y as f64) as f32;
        let relative_z = (z - floor_z as f64) as f32;
        self.sample_with_derivative(
            floor_x,
            floor_y,
            floor_z,
            relative_x,
            relative_y,
            relative_z,
            derivative_out,
        )
    }

    /// `sampleAndLerp`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sample_and_lerp(
        &self,
        x: i32,
        y: i32,
        z: i32,
        relative_x: f32,
        relative_y: f32,
        relative_z: f32,
        original_relative_y: f32,
    ) -> f32 {
        let n = &self.noise;
        let x0 = n.permute(x);
        let x1 = n.permute(x.wrapping_add(1));
        let xy00 = n.permute(x0.wrapping_add(y));
        let xy01 = n.permute(x0.wrapping_add(y).wrapping_add(1));
        let xy10 = n.permute(x1.wrapping_add(y));
        let xy11 = n.permute(x1.wrapping_add(y).wrapping_add(1));
        let d000 = grad_dot(
            n.permute(xy00.wrapping_add(z)),
            relative_x,
            relative_y,
            relative_z,
        );
        let d100 = grad_dot(
            n.permute(xy10.wrapping_add(z)),
            relative_x - 1.0,
            relative_y,
            relative_z,
        );
        let d010 = grad_dot(
            n.permute(xy01.wrapping_add(z)),
            relative_x,
            relative_y - 1.0,
            relative_z,
        );
        let d110 = grad_dot(
            n.permute(xy11.wrapping_add(z)),
            relative_x - 1.0,
            relative_y - 1.0,
            relative_z,
        );
        let d001 = grad_dot(
            n.permute(xy00.wrapping_add(z).wrapping_add(1)),
            relative_x,
            relative_y,
            relative_z - 1.0,
        );
        let d101 = grad_dot(
            n.permute(xy10.wrapping_add(z).wrapping_add(1)),
            relative_x - 1.0,
            relative_y,
            relative_z - 1.0,
        );
        let d011 = grad_dot(
            n.permute(xy01.wrapping_add(z).wrapping_add(1)),
            relative_x,
            relative_y - 1.0,
            relative_z - 1.0,
        );
        let d111 = grad_dot(
            n.permute(xy11.wrapping_add(z).wrapping_add(1)),
            relative_x - 1.0,
            relative_y - 1.0,
            relative_z - 1.0,
        );
        let x_alpha = mth::smoothstep(relative_x);
        let y_alpha = mth::smoothstep(original_relative_y);
        let z_alpha = mth::smoothstep(relative_z);
        mth::lerp3(
            x_alpha, y_alpha, z_alpha, d000, d100, d010, d110, d001, d101, d011, d111,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_with_derivative(
        &self,
        x: i32,
        y: i32,
        z: i32,
        xr: f32,
        yr: f32,
        zr: f32,
        derivative_out: &mut [f32; 3],
    ) -> f32 {
        let n = &self.noise;
        let x0 = n.permute(x);
        let x1 = n.permute(x.wrapping_add(1));
        let xy00 = n.permute(x0.wrapping_add(y));
        let xy01 = n.permute(x0.wrapping_add(y).wrapping_add(1));
        let xy10 = n.permute(x1.wrapping_add(y));
        let xy11 = n.permute(x1.wrapping_add(y).wrapping_add(1));
        let g000 = n.permute_to_grad(xy00.wrapping_add(z));
        let g100 = n.permute_to_grad(xy10.wrapping_add(z));
        let g010 = n.permute_to_grad(xy01.wrapping_add(z));
        let g110 = n.permute_to_grad(xy11.wrapping_add(z));
        let g001 = n.permute_to_grad(xy00.wrapping_add(z).wrapping_add(1));
        let g101 = n.permute_to_grad(xy10.wrapping_add(z).wrapping_add(1));
        let g011 = n.permute_to_grad(xy01.wrapping_add(z).wrapping_add(1));
        let g111 = n.permute_to_grad(xy11.wrapping_add(z).wrapping_add(1));
        let d000 = g000.dot(xr, yr, zr);
        let d100 = g100.dot(xr - 1.0, yr, zr);
        let d010 = g010.dot(xr, yr - 1.0, zr);
        let d110 = g110.dot(xr - 1.0, yr - 1.0, zr);
        let d001 = g001.dot(xr, yr, zr - 1.0);
        let d101 = g101.dot(xr - 1.0, yr, zr - 1.0);
        let d011 = g011.dot(xr, yr - 1.0, zr - 1.0);
        let d111 = g111.dot(xr - 1.0, yr - 1.0, zr - 1.0);
        let x_alpha = mth::smoothstep(xr);
        let y_alpha = mth::smoothstep(yr);
        let z_alpha = mth::smoothstep(zr);
        let gradients = [g000, g100, g010, g110, g001, g101, g011, g111];
        let lerp_gradient = |component: fn(&super::gradient_noise::Gradient) -> i32| {
            let c = |i: usize| component(&gradients[i]) as f32;
            mth::lerp3(
                x_alpha,
                y_alpha,
                z_alpha,
                c(0),
                c(1),
                c(2),
                c(3),
                c(4),
                c(5),
                c(6),
                c(7),
            )
        };
        let d1x = lerp_gradient(|g| g.x);
        let d1y = lerp_gradient(|g| g.y);
        let d1z = lerp_gradient(|g| g.z);
        let d2x = mth::lerp2(
            y_alpha,
            z_alpha,
            d100 - d000,
            d110 - d010,
            d101 - d001,
            d111 - d011,
        );
        let d2y = mth::lerp2(
            z_alpha,
            x_alpha,
            d010 - d000,
            d011 - d001,
            d110 - d100,
            d111 - d101,
        );
        let d2z = mth::lerp2(
            x_alpha,
            y_alpha,
            d001 - d000,
            d101 - d100,
            d011 - d010,
            d111 - d110,
        );
        let x_sd = mth::smoothstep_derivative(xr);
        let y_sd = mth::smoothstep_derivative(yr);
        let z_sd = mth::smoothstep_derivative(zr);
        derivative_out[0] += d1x + x_sd * d2x;
        derivative_out[1] += d1y + y_sd * d2y;
        derivative_out[2] += d1z + z_sd * d2z;
        mth::lerp3(
            x_alpha, y_alpha, z_alpha, d000, d100, d010, d110, d001, d101, d011, d111,
        )
    }
}

impl Noise for PerlinNoise {
    fn range(&self) -> Interval {
        Interval::of_symmetric(2.0)
    }

    fn get2(&self, x: f64, y: f64) -> f32 {
        self.get3(wrap(x), 0.0, wrap(y))
    }

    fn get3(&self, x: f64, y: f64, z: f64) -> f32 {
        let x = wrap(x) + self.noise.offset_x;
        let y = wrap(y) + self.noise.offset_y;
        let z = wrap(z) + self.noise.offset_z;
        let floor_x = mth::floor(x);
        let floor_y = mth::floor(y);
        let floor_z = mth::floor(z);
        let relative_x = (x - floor_x as f64) as f32;
        let relative_y = (y - floor_y as f64) as f32;
        let relative_z = (z - floor_z as f64) as f32;
        self.sample_and_lerp(
            floor_x, floor_y, floor_z, relative_x, relative_y, relative_z, relative_y,
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
        let mut corners = CachedCorners::default();
        let mut index = 0;
        for index_z in 0..volume.size_z {
            let z = wrap(volume.block_z(index_z) as f64 * xz_scale) + self.noise.offset_z;
            let floor_z = mth::floor(z);
            let relative_z = (z - floor_z as f64) as f32;
            let alpha_z = mth::smoothstep(relative_z);
            for index_x in 0..volume.size_x {
                let x = wrap(volume.block_x(index_x) as f64 * xz_scale) + self.noise.offset_x;
                let floor_x = mth::floor(x);
                let relative_x = (x - floor_x as f64) as f32;
                let x0 = self.noise.permute(floor_x);
                let x1 = self.noise.permute(floor_x.wrapping_add(1));
                let alpha_x = mth::smoothstep(relative_x);
                let mut last_floor_y = i32::MIN;
                for index_y in 0..volume.size_y {
                    let y = wrap(volume.block_y(index_y) as f64 * y_scale) + self.noise.offset_y;
                    let floor_y = mth::floor(y);
                    let relative_y = (y - floor_y as f64) as f32;
                    let alpha_y = mth::smoothstep(relative_y);
                    if last_floor_y != floor_y {
                        corners.load(
                            &self.noise,
                            x0,
                            x1,
                            floor_y,
                            floor_z,
                            relative_x,
                            relative_z,
                        );
                        last_floor_y = floor_y;
                    }
                    buffer.add_to(
                        index,
                        amplitude * corners.lerp(alpha_x, alpha_y, alpha_z, relative_y),
                    );
                    index += 1;
                }
            }
        }
    }
}
