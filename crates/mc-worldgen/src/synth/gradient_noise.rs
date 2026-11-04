//! `synth.GradientNoise`: shared permutation table, offsets and gradient helpers of the Perlin/Simplex noises.

use mc_core::random::RandomSource;

use crate::util::mth;

/// `GradientNoise.Gradient`.
#[derive(Clone, Copy)]
pub(super) struct Gradient {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Gradient {
    /// `dot(double, double, double)`.
    #[inline]
    pub fn dot_f64(self, x: f64, y: f64, z: f64) -> f64 {
        self.x as f64 * x + self.y as f64 * y + self.z as f64 * z
    }

    /// `dot(float, float, float)`.
    #[inline]
    pub fn dot(self, x: f32, y: f32, z: f32) -> f32 {
        self.x as f32 * x + self.y as f32 * y + self.z as f32 * z
    }

    #[inline]
    pub fn dot_xz(self, x: f32, z: f32) -> f32 {
        self.x as f32 * x + self.z as f32 * z
    }
}

const fn g(x: i32, y: i32, z: i32) -> Gradient {
    Gradient { x, y, z }
}

pub(super) const GRADIENT: [Gradient; 16] = [
    g(1, 1, 0),
    g(-1, 1, 0),
    g(1, -1, 0),
    g(-1, -1, 0),
    g(1, 0, 1),
    g(-1, 0, 1),
    g(1, 0, -1),
    g(-1, 0, -1),
    g(0, 1, 1),
    g(0, -1, 1),
    g(0, 1, -1),
    g(0, -1, -1),
    g(1, 1, 0),
    g(0, -1, 1),
    g(-1, 1, 0),
    g(0, -1, -1),
];

/// `Math.nextDown(1.6777216E7)`.
const HALF_ROUND_OFF: f64 = f64::from_bits(16777216.0f64.to_bits() - 1);

/// `GradientNoise.wrap` (`SharedConstants.DEBUG_ENABLE_FARLANDS` is false).
#[inline]
pub(super) fn wrap(x: f64) -> f64 {
    if (-HALF_ROUND_OFF..HALF_ROUND_OFF).contains(&x) {
        x
    } else {
        x - (x / 3.3554432E7 + 0.5).floor() * 3.3554432E7
    }
}

/// The state `GradientNoise` gives its subclasses.
#[derive(Clone)]
pub(super) struct GradientNoise {
    perms: [u8; 256],
    pub offset_x: f64,
    pub offset_y: f64,
    pub offset_z: f64,
}

impl GradientNoise {
    pub fn new(random: &mut dyn RandomSource, noise_offset_scale: f64) -> GradientNoise {
        let offset_x = random.next_double() * noise_offset_scale;
        let offset_y = random.next_double() * noise_offset_scale;
        let offset_z = random.next_double() * noise_offset_scale;
        let mut perms = [0u8; 256];
        for (i, perm) in perms.iter_mut().enumerate() {
            *perm = i as u8;
        }
        for i in 0..256 {
            let offset = random.next_int_bounded(256 - i as i32) as usize;
            perms.swap(i, offset + i);
        }
        GradientNoise {
            perms,
            offset_x,
            offset_y,
            offset_z,
        }
    }

    #[inline]
    pub fn permute(&self, x: i32) -> i32 {
        self.perms[(x & 0xFF) as usize] as i32
    }

    #[inline]
    pub fn permute_to_grad(&self, x: i32) -> Gradient {
        GRADIENT[(self.permute(x) & 15) as usize]
    }
}

/// `GradientNoise.gradDot`.
#[inline]
pub(super) fn grad_dot(hash: i32, x: f32, y: f32, z: f32) -> f32 {
    GRADIENT[(hash & 15) as usize].dot(x, y, z)
}

/// The eight per-cell xz dot products and y gradients that the `addToVolume` loops of `PerlinNoise` and
/// `SmearedPerlinNoise` cache while `floorY` stays the same. Corner `i` is `x + 2 * y + 4 * z` offset.
#[derive(Default)]
pub(super) struct CachedCorners {
    d_xz: [f32; 8],
    g_y: [f32; 8],
}

impl CachedCorners {
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &mut self,
        noise: &GradientNoise,
        x0: i32,
        x1: i32,
        floor_y: i32,
        floor_z: i32,
        rel_x: f32,
        rel_z: f32,
    ) {
        let xy = [
            noise.permute(x0.wrapping_add(floor_y)),
            noise.permute(x1.wrapping_add(floor_y)),
            noise.permute(x0.wrapping_add(floor_y).wrapping_add(1)),
            noise.permute(x1.wrapping_add(floor_y).wrapping_add(1)),
        ];
        for i in 0..8 {
            let z_hi = i >> 2 == 1;
            let z = floor_z.wrapping_add(z_hi as i32);
            let gradient = noise.permute_to_grad(xy[i & 3].wrapping_add(z));
            let x = if i & 1 == 1 { rel_x - 1.0 } else { rel_x };
            let z = if z_hi { rel_z - 1.0 } else { rel_z };
            self.d_xz[i] = gradient.dot_xz(x, z);
            self.g_y[i] = gradient.y as f32;
        }
    }

    /// `lerp3(alphaX, alphaY, alphaZ, d000xz + g000y * relativeY, ...)`.
    pub fn lerp(&self, alpha_x: f32, alpha_y: f32, alpha_z: f32, rel_y: f32) -> f32 {
        let d =
            |i: usize| self.d_xz[i] + self.g_y[i] * if i & 2 == 2 { rel_y - 1.0 } else { rel_y };
        mth::lerp3(
            alpha_x,
            alpha_y,
            alpha_z,
            d(0),
            d(1),
            d(2),
            d(3),
            d(4),
            d(5),
            d(6),
            d(7),
        )
    }
}
