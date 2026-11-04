//! net.minecraft.world.level.levelgen.synth: gradient noises and noise stacks.

pub mod blended_noise;
mod gradient_noise;
pub mod legacy_fbm_initializer;
pub mod noise_stack;
pub mod normal_noise;
pub mod perlin_noise;
pub mod simplex_noise;
pub mod smeared_perlin_noise;

pub use blended_noise::BlendedNoise;
pub use noise_stack::{NoiseStack, NoiseStackBuilder};
pub use normal_noise::{NoiseParameters, NormalNoise, NormalNoiseBuilder, Normalization};
pub use perlin_noise::PerlinNoise;
pub use simplex_noise::SimplexNoise;
pub use smeared_perlin_noise::SmearedPerlinNoise;

use crate::densityfunction::sampler::{DensityBuffer, DensityVolume};
use crate::util::Interval;

/// `synth.Noise`: a float-valued noise field over doubles.
pub trait Noise: Send + Sync {
    fn range(&self) -> Interval;

    /// `get(double x, double y)` (2D).
    fn get2(&self, x: f64, y: f64) -> f32;

    /// `get(double x, double y, double z)` (3D).
    fn get3(&self, x: f64, y: f64, z: f64) -> f32;

    /// `Noise.addToVolume`: adds `amplitude * get3(blockX * xz_scale, blockY * y_scale, blockZ * xz_scale)`
    /// to every cell of `buffer`. The default is the naive loop; Perlin-based noises override it with the
    /// vanilla gradient-caching variant, whose float rounding differs slightly from the point version.
    fn add_to_volume(
        &self,
        buffer: &mut DensityBuffer,
        volume: &DensityVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        let mut index = 0;
        for index_z in 0..volume.size_z {
            let z = volume.block_z(index_z) as f64 * xz_scale;
            for index_x in 0..volume.size_x {
                let x = volume.block_x(index_x) as f64 * xz_scale;
                for index_y in 0..volume.size_y {
                    let y = volume.block_y(index_y) as f64 * y_scale;
                    buffer.add_to(index, amplitude * self.get3(x, y, z));
                    index += 1;
                }
            }
        }
    }
}
