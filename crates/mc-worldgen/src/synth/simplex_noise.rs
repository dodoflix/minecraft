//! `synth.SimplexNoise`.

use mc_core::random::RandomSource;

use super::Noise;
use super::gradient_noise::{GRADIENT, GradientNoise};
use crate::util::{Interval, mth};

const SQRT_3: f64 = 1.7320508075688772;
const F2: f64 = 0.5 * (SQRT_3 - 1.0);
const G2: f64 = (3.0 - SQRT_3) / 6.0;

#[derive(Clone)]
pub struct SimplexNoise {
    noise: GradientNoise,
}

impl SimplexNoise {
    pub const STANDARD_DEVIATION: f64 = 0.42544;

    /// `SimplexNoise(RandomSource)`.
    pub fn with_default_offset(random: &mut dyn RandomSource) -> SimplexNoise {
        SimplexNoise {
            noise: GradientNoise::new(random, 256.0),
        }
    }

    /// `SimplexNoise(RandomSource, boolean discardNoiseOffset)`.
    pub fn new(random: &mut dyn RandomSource, discard_noise_offset: bool) -> SimplexNoise {
        SimplexNoise {
            noise: GradientNoise::new(random, if discard_noise_offset { 0.0 } else { 256.0 }),
        }
    }

    fn corner_noise_3d(index: i32, x: f64, y: f64, z: f64, base: f64) -> f64 {
        let t0 = base - x * x - y * y - z * z;
        if t0 < 0.0 {
            0.0
        } else {
            let t0 = t0 * t0;
            t0 * t0 * GRADIENT[index as usize].dot_f64(x, y, z)
        }
    }
}

impl Noise for SimplexNoise {
    fn range(&self) -> Interval {
        Interval::of_symmetric(2.0)
    }

    fn get2(&self, xin: f64, yin: f64) -> f32 {
        let n = &self.noise;
        let xin = xin + n.offset_x;
        let yin = yin + n.offset_y;
        let s = (xin + yin) * F2;
        let i = mth::floor(xin + s);
        let j = mth::floor(yin + s);
        let t = i.wrapping_add(j) as f64 * G2;
        let x0 = xin - (i as f64 - t);
        let y0 = yin - (j as f64 - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - i1 as f64 + G2;
        let y1 = y0 - j1 as f64 + G2;
        let x2 = x0 - 1.0 + 2.0 * G2;
        let y2 = y0 - 1.0 + 2.0 * G2;
        let ii = i & 0xFF;
        let jj = j & 0xFF;
        let gi0 = n.permute(ii + n.permute(jj)) % 12;
        let gi1 = n.permute(ii + i1 + n.permute(jj + j1)) % 12;
        let gi2 = n.permute(ii + 1 + n.permute(jj + 1)) % 12;
        let n0 = Self::corner_noise_3d(gi0, x0, y0, 0.0, 0.5);
        let n1 = Self::corner_noise_3d(gi1, x1, y1, 0.0, 0.5);
        let n2 = Self::corner_noise_3d(gi2, x2, y2, 0.0, 0.5);
        (70.0 * (n0 + n1 + n2)) as f32
    }

    fn get3(&self, xin: f64, yin: f64, zin: f64) -> f32 {
        let n = &self.noise;
        let xin = xin + n.offset_x;
        let yin = yin + n.offset_y;
        let zin = zin + n.offset_z;
        let s = (xin + yin + zin) * 0.3333333333333333;
        let i = mth::floor(xin + s);
        let j = mth::floor(yin + s);
        let k = mth::floor(zin + s);
        let t = i.wrapping_add(j).wrapping_add(k) as f64 * 0.16666666666666666;
        let x0 = xin - (i as f64 - t);
        let y0 = yin - (j as f64 - t);
        let z0 = zin - (k as f64 - t);
        let ((i1, j1, k1), (i2, j2, k2)) = if x0 >= y0 {
            if y0 >= z0 {
                ((1, 0, 0), (1, 1, 0))
            } else if x0 >= z0 {
                ((1, 0, 0), (1, 0, 1))
            } else {
                ((0, 0, 1), (1, 0, 1))
            }
        } else if y0 < z0 {
            ((0, 0, 1), (0, 1, 1))
        } else if x0 < z0 {
            ((0, 1, 0), (0, 1, 1))
        } else {
            ((0, 1, 0), (1, 1, 0))
        };
        let x1 = x0 - i1 as f64 + 0.16666666666666666;
        let y1 = y0 - j1 as f64 + 0.16666666666666666;
        let z1 = z0 - k1 as f64 + 0.16666666666666666;
        let x2 = x0 - i2 as f64 + 0.3333333333333333;
        let y2 = y0 - j2 as f64 + 0.3333333333333333;
        let z2 = z0 - k2 as f64 + 0.3333333333333333;
        let x3 = x0 - 1.0 + 0.5;
        let y3 = y0 - 1.0 + 0.5;
        let z3 = z0 - 1.0 + 0.5;
        let ii = i & 0xFF;
        let jj = j & 0xFF;
        let kk = k & 0xFF;
        let gi0 = n.permute(ii + n.permute(jj + n.permute(kk))) % 12;
        let gi1 = n.permute(ii + i1 + n.permute(jj + j1 + n.permute(kk + k1))) % 12;
        let gi2 = n.permute(ii + i2 + n.permute(jj + j2 + n.permute(kk + k2))) % 12;
        let gi3 = n.permute(ii + 1 + n.permute(jj + 1 + n.permute(kk + 1))) % 12;
        let n0 = Self::corner_noise_3d(gi0, x0, y0, z0, 0.6);
        let n1 = Self::corner_noise_3d(gi1, x1, y1, z1, 0.6);
        let n2 = Self::corner_noise_3d(gi2, x2, y2, z2, 0.6);
        let n3 = Self::corner_noise_3d(gi3, x3, y3, z3, 0.6);
        (32.0 * (n0 + n1 + n2 + n3)) as f32
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sqrt_3_literal_is_java_math_sqrt_3() {
        assert_eq!(super::SQRT_3, 3.0f64.sqrt());
    }
}
