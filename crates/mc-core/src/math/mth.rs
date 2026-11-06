//! Port of `net.minecraft.util.Mth`. Overloaded Java methods carry a type suffix on every variant.
//! Skipped: `Quaternionf`/`Fraction`/`Vector3fc` items (`rotationAroundAxis`, `mulAndTruncate`, axis constants).
use std::sync::LazyLock;
use uuid::Uuid;

use crate::math::atan_tables::{ASIN_TAB_BITS, COS_TAB_BITS};
use crate::math::java::{
    self, floor_div_i32, floor_div_i64, floor_mod_i32, max_f32, max_f64, min_f32, min_f64,
};
use crate::phys::{AABB, Vec3};
use crate::pos::Vec3i;
use crate::random::RandomSource;

pub const PI: f32 = std::f64::consts::PI as f32;
pub const HALF_PI: f32 = (std::f64::consts::PI / 2.0) as f32;
pub const TWO_PI: f32 = (std::f64::consts::PI * 2.0) as f32;
pub const DEG_TO_RAD: f32 = (std::f64::consts::PI / 180.0) as f32;
pub const RAD_TO_DEG: f32 = 180.0f32 / PI;
pub const EPSILON: f32 = 1.0E-5;
/// `sqrt(2.0F)`
#[allow(clippy::approx_constant)]
pub const SQRT_OF_TWO: f32 = 1.4142135;

const SIN_MASK: i64 = 65535;
const COS_OFFSET: f64 = 16384.0;
const SIN_SCALE: f64 = 10430.378350470453;
const FRAC_BIAS: f64 = f64::from_bits(4805340802404319232);

static SIN: LazyLock<Box<[f32]>> = LazyLock::new(|| {
    (0..65536)
        .map(|i| (f64::from(i) / SIN_SCALE).sin() as f32)
        .collect()
});

const MULTIPLY_DE_BRUIJN_BIT_POSITION: [i32; 32] = [
    0, 1, 28, 2, 29, 14, 24, 3, 30, 22, 20, 15, 25, 17, 4, 8, 31, 27, 13, 23, 21, 19, 16, 7, 26,
    12, 18, 6, 11, 5, 10, 9,
];

/// (ASIN_TAB, COS_TAB)
static ATAN_TABLES: LazyLock<(Box<[f64]>, Box<[f64]>)> = LazyLock::new(|| {
    let conv = |bits: &[u64; 257]| {
        bits.iter()
            .map(|b| f64::from_bits(*b))
            .collect::<Box<[f64]>>()
    };
    (conv(&ASIN_TAB_BITS), conv(&COS_TAB_BITS))
});

pub fn sin(i: f64) -> f32 {
    SIN[((i * SIN_SCALE) as i64 & SIN_MASK) as usize]
}

pub fn cos(i: f64) -> f32 {
    SIN[((i * SIN_SCALE + COS_OFFSET) as i64 & SIN_MASK) as usize]
}

pub fn sqrt(x: f32) -> f32 {
    f64::from(x).sqrt() as f32
}

pub fn floor_f32(v: f32) -> i32 {
    v.floor() as i32
}

pub fn floor_f64(v: f64) -> i32 {
    v.floor() as i32
}

pub fn lfloor(v: f64) -> i64 {
    v.floor() as i64
}

pub fn abs_f32(v: f32) -> f32 {
    v.abs()
}

pub fn abs_i32(v: i32) -> i32 {
    v.wrapping_abs()
}

pub fn ceil_f32(v: f32) -> i32 {
    v.ceil() as i32
}

pub fn ceil_f64(v: f64) -> i32 {
    v.ceil() as i32
}

pub fn ceil_long(v: f64) -> i64 {
    v.ceil() as i64
}

pub fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

pub fn clamp_i64(value: i64, min: i64, max: i64) -> i64 {
    value.max(min).min(max)
}

pub fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else {
        min_f32(value, max)
    }
}

pub fn clamp_f64(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else {
        min_f64(value, max)
    }
}

pub fn clamped_lerp_f64(factor: f64, min: f64, max: f64) -> f64 {
    if factor < 0.0 {
        min
    } else if factor > 1.0 {
        max
    } else {
        lerp_f64(factor, min, max)
    }
}

pub fn clamped_lerp_f32(factor: f32, min: f32, max: f32) -> f32 {
    if factor < 0.0 {
        min
    } else if factor > 1.0 {
        max
    } else {
        lerp_f32(factor, min, max)
    }
}

pub fn abs_max_i32(a: i32, b: i32) -> i32 {
    a.wrapping_abs().max(b.wrapping_abs())
}

pub fn abs_max_f32(a: f32, b: f32) -> f32 {
    max_f32(a.abs(), b.abs())
}

pub fn abs_max_f64(a: f64, b: f64) -> f64 {
    max_f64(a.abs(), b.abs())
}

pub fn chessboard_distance(x0: i32, z0: i32, x1: i32, z1: i32) -> i32 {
    abs_max_i32(x1.wrapping_sub(x0), z1.wrapping_sub(z0))
}

pub fn floor_div(a: i32, b: i32) -> i32 {
    floor_div_i32(a, b)
}

/// `Mth.nextInt(random, min, maxInclusive)`
pub fn next_int(random: &mut dyn RandomSource, min_inclusive: i32, max_inclusive: i32) -> i32 {
    if min_inclusive >= max_inclusive {
        min_inclusive
    } else {
        random
            .next_int_bounded(max_inclusive.wrapping_sub(min_inclusive).wrapping_add(1))
            .wrapping_add(min_inclusive)
    }
}

pub fn next_float(random: &mut dyn RandomSource, min: f32, max: f32) -> f32 {
    if min >= max {
        min
    } else {
        random.next_float() * (max - min) + min
    }
}

pub fn next_double(random: &mut dyn RandomSource, min: f64, max: f64) -> f64 {
    if min >= max {
        min
    } else {
        random.next_double() * (max - min) + min
    }
}

pub fn equal_f32(a: f32, b: f32) -> bool {
    (b - a).abs() < 1.0E-5
}

pub fn equal_f64(a: f64, b: f64) -> bool {
    (b - a).abs() < f64::from(1.0E-5f32)
}

pub fn positive_modulo_i32(input: i32, modulus: i32) -> i32 {
    floor_mod_i32(input, modulus)
}

pub fn positive_modulo_f32(input: f32, modulus: f32) -> f32 {
    (input % modulus + modulus) % modulus
}

pub fn positive_modulo_f64(input: f64, modulus: f64) -> f64 {
    (input % modulus + modulus) % modulus
}

pub fn is_multiple_of(dividend: i32, divisor: i32) -> bool {
    dividend.wrapping_rem(divisor) == 0
}

pub fn pack_degrees(angle: f32) -> i8 {
    floor_f32(angle * 256.0 / 360.0) as i8
}

pub fn unpack_degrees(rot: i8) -> f32 {
    (i32::from(rot) * 360) as f32 / 256.0
}

pub fn wrap_degrees_i32(angle: i32) -> i32 {
    let mut n = angle % 360;
    if n >= 180 {
        n -= 360;
    }
    if n < -180 {
        n += 360;
    }
    n
}

pub fn wrap_degrees_i64(angle: i64) -> f32 {
    let mut n = (angle % 360) as f32;
    if n >= 180.0 {
        n -= 360.0;
    }
    if n < -180.0 {
        n += 360.0;
    }
    n
}

pub fn wrap_degrees_f32(angle: f32) -> f32 {
    let mut n = angle % 360.0;
    if n >= 180.0 {
        n -= 360.0;
    }
    if n < -180.0 {
        n += 360.0;
    }
    n
}

pub fn wrap_degrees_f64(angle: f64) -> f64 {
    let mut n = angle % 360.0;
    if n >= 180.0 {
        n -= 360.0;
    }
    if n < -180.0 {
        n += 360.0;
    }
    n
}

pub fn wrap_degrees90(angle: f32) -> f32 {
    let mut n = angle % 90.0;
    if n >= 45.0 {
        n -= 90.0;
    }
    if n < -45.0 {
        n += 90.0;
    }
    n
}

pub fn degrees_difference(from_angle: f32, to_angle: f32) -> f32 {
    wrap_degrees_f32(to_angle - from_angle)
}

pub fn degrees_difference_abs(angle_a: f32, angle_b: f32) -> f32 {
    degrees_difference(angle_a, angle_b).abs()
}

pub fn rotate_if_necessary(base_angle: f32, target_angle: f32, max_angle_diff: f32) -> f32 {
    let delta = degrees_difference(base_angle, target_angle);
    let clamped = clamp_f32(delta, -max_angle_diff, max_angle_diff);
    target_angle - clamped
}

pub fn approach(current: f32, target: f32, increment: f32) -> f32 {
    let increment = increment.abs();
    if current < target {
        clamp_f32(current + increment, current, target)
    } else {
        clamp_f32(current - increment, target, current)
    }
}

pub fn approach_degrees(current: f32, target: f32, increment: f32) -> f32 {
    let difference = degrees_difference(current, target);
    approach(current, current + difference, increment)
}

/// `Mth.getInt(String, int)` (`NumberUtils.toInt`): `def` when unparsable.
pub fn get_int(input: &str, def: i32) -> i32 {
    java::parse_int(input).unwrap_or(def)
}

pub fn smallest_encompassing_power_of_two(input: i32) -> i32 {
    let mut result = input.wrapping_sub(1);
    result |= result >> 1;
    result |= result >> 2;
    result |= result >> 4;
    result |= result >> 8;
    result |= result >> 16;
    result.wrapping_add(1)
}

/// Panics (vanilla: `IllegalArgumentException`) on negative `item_count`.
pub fn smallest_square_side(item_count: i32) -> i32 {
    assert!(
        item_count >= 0,
        "itemCount must be greater than or equal to zero"
    );
    ceil_f64(f64::from(item_count).sqrt())
}

pub fn is_power_of_two_i32(input: i32) -> bool {
    input != 0 && (input & input.wrapping_sub(1)) == 0
}

pub fn is_power_of_two_i64(input: i64) -> bool {
    input != 0 && (input & input.wrapping_sub(1)) == 0
}

pub fn ceillog2(input: i32) -> i32 {
    let input = if is_power_of_two_i32(input) {
        input
    } else {
        smallest_encompassing_power_of_two(input)
    };
    MULTIPLY_DE_BRUIJN_BIT_POSITION[((i64::from(input) * 125613361 >> 27) as i32 & 31) as usize]
}

pub fn log2(input: i32) -> i32 {
    ceillog2(input) - i32::from(!is_power_of_two_i32(input))
}

pub fn frac_f32(num: f32) -> f32 {
    num - floor_f32(num) as f32
}

pub fn frac_f64(num: f64) -> f64 {
    num - lfloor(num) as f64
}

/// `Mth.getSeed(Vec3i)` (deprecated in vanilla).
pub fn get_seed(vec: &Vec3i) -> i64 {
    get_seed_xyz(vec.x, vec.y, vec.z)
}

/// `Mth.getSeed(int, int, int)` (deprecated in vanilla).
pub fn get_seed_xyz(x: i32, y: i32, z: i32) -> i64 {
    let mut seed =
        i64::from(x.wrapping_mul(3129871)) ^ i64::from(z).wrapping_mul(116129781) ^ i64::from(y);
    seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42317861)
        .wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

pub fn create_insecure_uuid(random: &mut dyn RandomSource) -> Uuid {
    let most = random.next_long() & -61441 | 16384;
    let least = random.next_long() & 4611686018427387903 | i64::MIN;
    Uuid::from_u64_pair(most as u64, least as u64)
}

pub fn inverse_lerp_f64(value: f64, min: f64, max: f64) -> f64 {
    (value - min) / (max - min)
}

pub fn inverse_lerp_f32(value: f32, min: f32, max: f32) -> f32 {
    (value - min) / (max - min)
}

pub fn ray_intersects_aabb(ray_start: &Vec3, ray_dir: &Vec3, aabb: &AABB) -> bool {
    let center_x = (aabb.min_x + aabb.max_x) * 0.5;
    let box_extent_x = (aabb.max_x - aabb.min_x) * 0.5;
    let diff_x = ray_start.x - center_x;
    if diff_x.abs() > box_extent_x && diff_x * ray_dir.x >= 0.0 {
        return false;
    }
    let center_y = (aabb.min_y + aabb.max_y) * 0.5;
    let box_extent_y = (aabb.max_y - aabb.min_y) * 0.5;
    let diff_y = ray_start.y - center_y;
    if diff_y.abs() > box_extent_y && diff_y * ray_dir.y >= 0.0 {
        return false;
    }
    let center_z = (aabb.min_z + aabb.max_z) * 0.5;
    let box_extent_z = (aabb.max_z - aabb.min_z) * 0.5;
    let diff_z = ray_start.z - center_z;
    if diff_z.abs() > box_extent_z && diff_z * ray_dir.z >= 0.0 {
        return false;
    }
    let aw_x = ray_dir.x.abs();
    let aw_y = ray_dir.y.abs();
    let aw_z = ray_dir.z.abs();
    let mut f = ray_dir.y * diff_z - ray_dir.z * diff_y;
    if f.abs() > box_extent_y * aw_z + box_extent_z * aw_y {
        return false;
    }
    f = ray_dir.z * diff_x - ray_dir.x * diff_z;
    if f.abs() > box_extent_x * aw_z + box_extent_z * aw_x {
        return false;
    }
    f = ray_dir.x * diff_y - ray_dir.y * diff_x;
    f.abs() < box_extent_x * aw_y + box_extent_y * aw_x
}

pub fn atan2(mut y: f64, mut x: f64) -> f64 {
    let d2 = x * x + y * y;
    if d2.is_nan() {
        return f64::NAN;
    }
    let neg_y = y < 0.0;
    if neg_y {
        y = -y;
    }
    let neg_x = x < 0.0;
    if neg_x {
        x = -x;
    }
    let steep = y > x;
    if steep {
        std::mem::swap(&mut x, &mut y);
    }
    let rinv = fast_inv_sqrt(d2);
    x *= rinv;
    y *= rinv;
    let yp = FRAC_BIAS + y;
    let index = yp.to_bits() as i32 as usize;
    let (asin_tab, cos_tab) = &*ATAN_TABLES;
    let phi = asin_tab[index];
    let c_phi = cos_tab[index];
    let s_phi = yp - FRAC_BIAS;
    let sd = y * c_phi - x * s_phi;
    let d = (6.0 + sd * sd) * sd * 0.16666666666666666;
    let mut theta = phi + d;
    if steep {
        theta = std::f64::consts::FRAC_PI_2 - theta;
    }
    if neg_x {
        theta = std::f64::consts::PI - theta;
    }
    if neg_y {
        theta = -theta;
    }
    theta
}

/// `org.joml.Math.invsqrt(float)`
pub fn inv_sqrt_f32(x: f32) -> f32 {
    1.0 / sqrt(x)
}

/// `org.joml.Math.invsqrt(double)`
pub fn inv_sqrt_f64(x: f64) -> f64 {
    1.0 / x.sqrt()
}

pub fn fast_inv_sqrt(x: f64) -> f64 {
    let xhalf = 0.5 * x;
    let i = (x.to_bits() as i64) >> 1;
    let x = f64::from_bits(6910469410427058090i64.wrapping_sub(i) as u64);
    x * (1.5 - xhalf * x * x)
}

pub fn fast_inv_cube_root(x: f32) -> f32 {
    let i = (x.to_bits() as i32) / 3;
    let mut y = f32::from_bits(1419967116i32.wrapping_sub(i) as u32);
    y = 0.6666667 * y + 1.0 / (3.0 * y * y * x);
    0.6666667 * y + 1.0 / (3.0 * y * y * x)
}

pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> i32 {
    hsv_to_argb(hue, saturation, value, 0)
}

/// Panics (vanilla: `RuntimeException`) when `hue` is out of range (negative or NaN-derived sector).
pub fn hsv_to_argb(hue: f32, saturation: f32, value: f32, alpha: i32) -> i32 {
    let h = (hue * 6.0) as i32 % 6;
    let f = hue * 6.0 - h as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - f * saturation);
    let t = value * (1.0 - (1.0 - f) * saturation);
    let (red, green, blue) = match h {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        5 => (value, p, q),
        _ => panic!(
            "Something went wrong when converting from HSV to RGB. Input was {hue}, {saturation}, {value}"
        ),
    };
    let c = |v: f32| clamp_i32((v * 255.0) as i32, 0, 255);
    // ARGB.color(alpha, r, g, b)
    (alpha & 0xFF) << 24 | (c(red) & 0xFF) << 16 | (c(green) & 0xFF) << 8 | c(blue) & 0xFF
}

pub fn murmur_hash3_mixer(mut hash: i32) -> i32 {
    hash ^= ((hash as u32) >> 16) as i32;
    hash = hash.wrapping_mul(-2048144789);
    hash ^= ((hash as u32) >> 13) as i32;
    hash = hash.wrapping_mul(-1028477387);
    hash ^ ((hash as u32) >> 16) as i32
}

/// `Mth.binarySearch(from, to, condition)`: first index in `[from, to)` where `condition` holds (else `to`).
pub fn binary_search(mut from: i32, to: i32, mut condition: impl FnMut(i32) -> bool) -> i32 {
    let mut len = to.wrapping_sub(from);
    while len > 0 {
        let half = len / 2;
        let middle = from.wrapping_add(half);
        if condition(middle) {
            len = half;
        } else {
            from = middle.wrapping_add(1);
            len -= half + 1;
        }
    }
    from
}

pub fn lerp_int(alpha1: f32, p0: i32, p1: i32) -> i32 {
    p0.wrapping_add(floor_f32(alpha1 * p1.wrapping_sub(p0) as f32))
}

pub fn lerp_discrete(alpha1: f32, p0: i32, p1: i32) -> i32 {
    let delta = p1.wrapping_sub(p0);
    p0.wrapping_add(floor_f32(alpha1 * delta.wrapping_sub(1) as f32))
        .wrapping_add(i32::from(alpha1 > 0.0))
}

pub fn lerp_f32(alpha1: f32, p0: f32, p1: f32) -> f32 {
    p0 + alpha1 * (p1 - p0)
}

pub fn lerp2_f32(alpha1: f32, alpha2: f32, x00: f32, x10: f32, x01: f32, x11: f32) -> f32 {
    lerp_f32(
        alpha2,
        lerp_f32(alpha1, x00, x10),
        lerp_f32(alpha1, x01, x11),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn lerp3_f32(
    alpha1: f32,
    alpha2: f32,
    alpha3: f32,
    x000: f32,
    x100: f32,
    x010: f32,
    x110: f32,
    x001: f32,
    x101: f32,
    x011: f32,
    x111: f32,
) -> f32 {
    lerp_f32(
        alpha3,
        lerp2_f32(alpha1, alpha2, x000, x100, x010, x110),
        lerp2_f32(alpha1, alpha2, x001, x101, x011, x111),
    )
}

pub fn lerp_vec3(alpha: f64, p1: &Vec3, p2: &Vec3) -> Vec3 {
    Vec3 {
        x: lerp_f64(alpha, p1.x, p2.x),
        y: lerp_f64(alpha, p1.y, p2.y),
        z: lerp_f64(alpha, p1.z, p2.z),
    }
}

pub fn lerp_f64(alpha1: f64, p0: f64, p1: f64) -> f64 {
    p0 + alpha1 * (p1 - p0)
}

pub fn lerp2_f64(alpha1: f64, alpha2: f64, x00: f64, x10: f64, x01: f64, x11: f64) -> f64 {
    lerp_f64(
        alpha2,
        lerp_f64(alpha1, x00, x10),
        lerp_f64(alpha1, x01, x11),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn lerp3_f64(
    alpha1: f64,
    alpha2: f64,
    alpha3: f64,
    x000: f64,
    x100: f64,
    x010: f64,
    x110: f64,
    x001: f64,
    x101: f64,
    x011: f64,
    x111: f64,
) -> f64 {
    lerp_f64(
        alpha3,
        lerp2_f64(alpha1, alpha2, x000, x100, x010, x110),
        lerp2_f64(alpha1, alpha2, x001, x101, x011, x111),
    )
}

pub fn catmullrom(alpha: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * alpha
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * alpha * alpha
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * alpha * alpha * alpha)
}

pub fn smoothstep(x: f32) -> f32 {
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

pub fn smoothstep_derivative(x: f32) -> f32 {
    30.0 * x * x * (x - 1.0) * (x - 1.0)
}

pub fn sign(number: f64) -> i32 {
    if number == 0.0 {
        0
    } else if number > 0.0 {
        1
    } else {
        -1
    }
}

pub fn rot_lerp_f32(a: f32, from: f32, to: f32) -> f32 {
    from + a * wrap_degrees_f32(to - from)
}

pub fn rot_lerp_f64(a: f64, from: f64, to: f64) -> f64 {
    from + a * wrap_degrees_f64(to - from)
}

pub fn rot_lerp_rad(a: f32, from: f32, to: f32) -> f32 {
    let mut diff = to - from;
    while diff < -PI {
        diff += TWO_PI;
    }
    while diff >= PI {
        diff -= TWO_PI;
    }
    from + a * diff
}

pub fn triangle_wave(index: f32, period: f32) -> f32 {
    ((index % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}

pub fn square_f32(x: f32) -> f32 {
    x * x
}

pub fn cube(x: f32) -> f32 {
    x * x * x
}

pub fn square_f64(x: f64) -> f64 {
    x * x
}

pub fn square_i32(x: i32) -> i32 {
    x.wrapping_mul(x)
}

pub fn square_i64(x: i64) -> i64 {
    x.wrapping_mul(x)
}

pub fn clamped_map_f64(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    clamped_lerp_f64(inverse_lerp_f64(value, from_min, from_max), to_min, to_max)
}

pub fn clamped_map_f32(value: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32 {
    clamped_lerp_f32(inverse_lerp_f32(value, from_min, from_max), to_min, to_max)
}

pub fn map_f64(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    lerp_f64(inverse_lerp_f64(value, from_min, from_max), to_min, to_max)
}

pub fn map_f32(value: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32 {
    lerp_f32(inverse_lerp_f32(value, from_min, from_max), to_min, to_max)
}

pub fn round_toward_i32(input: i32, multiple: i32) -> i32 {
    positive_ceil_div_i32(input, multiple).wrapping_mul(multiple)
}

pub fn round_toward_i64(input: i64, multiple: i64) -> i64 {
    positive_ceil_div_i64(input, multiple).wrapping_mul(multiple)
}

pub fn positive_ceil_div_i32(input: i32, divisor: i32) -> i32 {
    floor_div_i32(input.wrapping_neg(), divisor).wrapping_neg()
}

pub fn positive_ceil_div_i64(input: i64, divisor: i64) -> i64 {
    floor_div_i64(input.wrapping_neg(), divisor).wrapping_neg()
}

pub fn random_between_inclusive(
    random: &mut dyn RandomSource,
    min: i32,
    max_inclusive: i32,
) -> i32 {
    random
        .next_int_bounded(max_inclusive.wrapping_sub(min).wrapping_add(1))
        .wrapping_add(min)
}

pub fn random_between(random: &mut dyn RandomSource, min: f32, max_exclusive: f32) -> f32 {
    random.next_float() * (max_exclusive - min) + min
}

pub fn normal(random: &mut dyn RandomSource, mean: f32, deviation: f32) -> f32 {
    mean + random.next_gaussian() as f32 * deviation
}

pub fn length_squared_f64(x: f64, y: f64) -> f64 {
    x * x + y * y
}

pub fn length_squared_f32(x: f32, y: f32) -> f32 {
    x * x + y * y
}

pub fn length_f64(x: f64, y: f64) -> f64 {
    length_squared_f64(x, y).sqrt()
}

pub fn length_f32(x: f32, y: f32) -> f32 {
    f64::from(length_squared_f32(x, y)).sqrt() as f32
}

pub fn length_squared_xyz_f64(x: f64, y: f64, z: f64) -> f64 {
    x * x + y * y + z * z
}

pub fn length_xyz_f64(x: f64, y: f64, z: f64) -> f64 {
    length_squared_xyz_f64(x, y, z).sqrt()
}

pub fn length_xyz_f32(x: f32, y: f32, z: f32) -> f32 {
    sqrt(length_squared_xyz_f32(x, y, z))
}

pub fn length_squared_xyz_f32(x: f32, y: f32, z: f32) -> f32 {
    x * x + y * y + z * z
}

pub fn quantize(value: f64, quantize_resolution: i32) -> i32 {
    floor_f64(value / f64::from(quantize_resolution)).wrapping_mul(quantize_resolution)
}

pub fn out_from_origin(origin: i32, lower_bound: i32, upper_bound: i32) -> Vec<i32> {
    out_from_origin_step(origin, lower_bound, upper_bound, 1)
}

/// `Mth.outFromOrigin(origin, lowerBound, upperBound, stepSize)` collected eagerly.
/// Panics (vanilla: `IllegalArgumentException`) on invalid bounds / step.
pub fn out_from_origin_step(
    origin: i32,
    lower_bound: i32,
    upper_bound: i32,
    step_size: i32,
) -> Vec<i32> {
    assert!(
        lower_bound <= upper_bound,
        "upperBound {upper_bound} expected to be > lowerBound {lower_bound}"
    );
    assert!(
        step_size >= 1,
        "step size expected to be >= 1, was {step_size}"
    );
    let o = clamp_i32(origin, lower_bound, upper_bound);
    let has_next = |cursor: i32| {
        let d = o.wrapping_sub(cursor).wrapping_abs();
        o.wrapping_sub(d) >= lower_bound || o.wrapping_add(d) <= upper_bound
    };
    let next = |cursor: i32| {
        let prev_neg = cursor <= o;
        let d = o.wrapping_sub(cursor).wrapping_abs();
        let can_move_positive = o.wrapping_add(d).wrapping_add(step_size) <= upper_bound;
        if !prev_neg || !can_move_positive {
            let attempted = o
                .wrapping_sub(d)
                .wrapping_sub(if prev_neg { step_size } else { 0 });
            if attempted >= lower_bound {
                return attempted;
            }
        }
        o.wrapping_add(d).wrapping_add(step_size)
    };
    let mut out = Vec::new();
    let mut cursor = o;
    while has_next(cursor) {
        out.push(cursor);
        cursor = next(cursor);
    }
    out
}

pub fn wobble(coord: f64) -> f64 {
    let mut random =
        crate::random::SingleThreadedRandomSource::new(floor_f64(coord * 3000.0) as i64);
    coord + (2.0 * random.next_double() - 1.0) * 1.0E-7 / 2.0
}
