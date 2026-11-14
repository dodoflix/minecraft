//! Scalar helpers of `org.joml.Math` with JOML's default options (no fast math, no sine lookup, no `Math.fma`), shared by
//! the float matrix and quaternion classes of this module tree.

/// `Math.fma(a, b, c)`: with `joml.useMathFma` off (the default) JOML computes `a * b + c` in two float roundings.
pub(super) fn fma(a: f32, b: f32, c: f32) -> f32 {
    a * b + c
}

/// `Math.sin(float)`: `(float) java.lang.Math.sin(rad)`.
pub(super) fn sin(rad: f32) -> f32 {
    f64::from(rad).sin() as f32
}

/// `Math.sqrt(float)`: `(float) java.lang.Math.sqrt(r)`.
pub(super) fn sqrt(r: f32) -> f32 {
    f64::from(r).sqrt() as f32
}

/// `Math.invsqrt(float)`.
pub(super) fn invsqrt(r: f32) -> f32 {
    1.0 / sqrt(r)
}

/// `Math.cosFromSin(float, float)` (the non fast-math branch).
pub(super) fn cos_from_sin(sin: f32, angle: f32) -> f32 {
    const PI: f32 = std::f64::consts::PI as f32;
    const TAU: f32 = (std::f64::consts::PI * 2.0) as f32;
    let cos = sqrt(1.0 - sin * sin);
    let a = angle + (std::f64::consts::PI / 2.0) as f32;
    let mut b = a - (a / TAU) as i32 as f32 * TAU;
    if b < 0.0 {
        b += TAU;
    }
    if b >= PI { -cos } else { cos }
}

/// `Math.max(float, float)`: `a > b ? a : b` (not `java.lang.Math.max`: a NaN `a` yields `b`).
pub(super) fn max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// `Math.absEqualsOne(float)`.
pub(super) fn abs_equals_one(r: f32) -> bool {
    r.to_bits() & 0x7FFF_FFFF == 0x3F80_0000
}

/// `Math.isFinite(float)`: `abs(f) <= Float.MAX_VALUE`.
pub(super) fn is_finite(f: f32) -> bool {
    f.abs() <= f32::MAX
}

/// `(float) java.lang.Math.hypot(a, b)`. Java's fdlibm port works on `|a|` and `|b|`: an infinite argument wins over a
/// NaN, otherwise a NaN argument gives the positive NaN (the C library keeps the sign of the NaN operand).
pub(super) fn hypot(a: f32, b: f32) -> f32 {
    let (a, b) = (f64::from(a), f64::from(b));
    if a.is_infinite() || b.is_infinite() {
        f32::INFINITY
    } else if a.is_nan() || b.is_nan() {
        f32::NAN
    } else {
        a.hypot(b) as f32
    }
}

/// `Vector3f.rotateY(angle)` of `(x, y, z)` (joml 1.10.9: `x' = x * cos + z * sin`, `z' = -x * sin + z * cos`).
pub fn rotate_y(x: f32, y: f32, z: f32, angle: f32) -> (f32, f32, f32) {
    let sin = sin(angle);
    let cos = cos_from_sin(sin, angle);
    (x * cos + z * sin, y, -x * sin + z * cos)
}
