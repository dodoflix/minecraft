//! Java numeric semantics used by the levelgen code: `Interval` (net.minecraft.util.Interval),
//! `Math.min/max/signum/round` on floats and the `float` flavours of `Mth`.
//!
//! Everything here is `f32` exactly where vanilla is `float`; do not "improve" precision.

use std::hash::{Hash, Hasher};

// ---------------------------------------------------------------------------------------------
// java.lang.Math on floats
// ---------------------------------------------------------------------------------------------

/// `Math.min(float, float)`: NaN-propagating, `-0.0 < 0.0`.
#[inline]
pub fn jmin(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if b.is_nan() {
        return b;
    }
    if a == 0.0 && b == 0.0 {
        // -0.0 is "smaller"
        return if a.is_sign_negative() { a } else { b };
    }
    if a <= b { a } else { b }
}

/// `Math.max(float, float)`: NaN-propagating, `-0.0 < 0.0`.
#[inline]
pub fn jmax(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if b.is_nan() {
        return b;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a >= b { a } else { b }
}

/// `Math.signum(float)`: NaN -> NaN, +-0.0 -> same zero.
#[inline]
pub fn signum(x: f32) -> f32 {
    if x == 0.0 || x.is_nan() {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// `Math.round(float)` widened back to `float` (as `(float)Math.round(x)` would give you).
#[inline]
pub fn round_f32(x: f32) -> f32 {
    // Math.round(float) == floor(x + 0.5) saturated to int; NaN -> 0. f64 holds x + 0.5 exactly.
    ((x as f64 + 0.5).floor() as i32) as f32
}

/// `Math.floorMod(int, int)`.
#[inline]
pub fn floor_mod(a: i32, b: i32) -> i32 {
    a.wrapping_sub(floor_div(a, b).wrapping_mul(b))
}

/// `Math.floorDiv(int, int)`.
#[inline]
pub fn floor_div(a: i32, b: i32) -> i32 {
    let q = a.wrapping_div(b);
    if (a ^ b) < 0 && q.wrapping_mul(b) != a {
        q - 1
    } else {
        q
    }
}

// ---------------------------------------------------------------------------------------------
// net.minecraft.util.Mth (float flavours)
// ---------------------------------------------------------------------------------------------

pub mod mth {
    use super::*;

    #[inline]
    pub fn sqrt(x: f32) -> f32 {
        (x as f64).sqrt() as f32
    }
    /// `Mth.floor(double)`.
    #[inline]
    pub fn floor(v: f64) -> i32 {
        v.floor() as i32
    }
    /// `Mth.clamp(float, float, float)`.
    #[inline]
    pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
        if value < min { min } else { jmin(value, max) }
    }
    #[inline]
    pub fn clamp_i(value: i32, min: i32, max: i32) -> i32 {
        value.max(min).min(max)
    }
    #[inline]
    pub fn lerp(alpha: f32, p0: f32, p1: f32) -> f32 {
        p0 + alpha * (p1 - p0)
    }
    #[inline]
    pub fn lerp2(a1: f32, a2: f32, x00: f32, x10: f32, x01: f32, x11: f32) -> f32 {
        lerp(a2, lerp(a1, x00, x10), lerp(a1, x01, x11))
    }
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn lerp3(
        a1: f32,
        a2: f32,
        a3: f32,
        x000: f32,
        x100: f32,
        x010: f32,
        x110: f32,
        x001: f32,
        x101: f32,
        x011: f32,
        x111: f32,
    ) -> f32 {
        lerp(
            a3,
            lerp2(a1, a2, x000, x100, x010, x110),
            lerp2(a1, a2, x001, x101, x011, x111),
        )
    }
    #[inline]
    pub fn smoothstep(x: f32) -> f32 {
        x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
    }
    #[inline]
    pub fn smoothstep_derivative(x: f32) -> f32 {
        30.0 * x * x * (x - 1.0) * (x - 1.0)
    }
    #[inline]
    pub fn square(x: f32) -> f32 {
        x * x
    }
    #[inline]
    pub fn cube(x: f32) -> f32 {
        x * x * x
    }
    #[inline]
    pub fn length_squared2(x: f32, y: f32) -> f32 {
        x * x + y * y
    }
    #[inline]
    pub fn length2(x: f32, y: f32) -> f32 {
        (length_squared2(x, y) as f64).sqrt() as f32
    }
    #[inline]
    pub fn length_squared3(x: f32, y: f32, z: f32) -> f32 {
        x * x + y * y + z * z
    }
    #[inline]
    pub fn length3(x: f32, y: f32, z: f32) -> f32 {
        sqrt(length_squared3(x, y, z))
    }
    /// `Mth.positiveCeilDiv`.
    #[inline]
    pub fn positive_ceil_div(input: i32, divisor: i32) -> i32 {
        floor_div(input.wrapping_neg(), divisor).wrapping_neg()
    }
    /// `Mth.roundToward(int, int)`.
    #[inline]
    pub fn round_toward(input: i32, multiple: i32) -> i32 {
        positive_ceil_div(input, multiple).wrapping_mul(multiple)
    }
    /// `Mth.binarySearch(from, to, condition)`: first index in `[from, to)` where `condition` holds.
    pub fn binary_search(mut from: i32, to: i32, mut condition: impl FnMut(i32) -> bool) -> i32 {
        let mut len = to - from;
        while len > 0 {
            let half = len / 2;
            let middle = from + half;
            if condition(middle) {
                len = half;
            } else {
                from = middle + 1;
                len -= half + 1;
            }
        }
        from
    }
}

// ---------------------------------------------------------------------------------------------
// net.minecraft.util.Interval
// ---------------------------------------------------------------------------------------------

/// Closed float interval, or the "not an interval" marker [`Interval::NAI`] (both bounds NaN).
#[derive(Clone, Copy, Debug)]
pub struct Interval {
    min: f32,
    max: f32,
}

impl PartialEq for Interval {
    /// Java `Interval.equals`: `min == min && max == max` on primitives (so NaI != NaI by value, but
    /// the Java check short-circuits on identity; we treat NaI as equal to NaI).
    fn eq(&self, other: &Self) -> bool {
        (self.is_nai() && other.is_nai()) || (self.min == other.min && self.max == other.max)
    }
}

impl Interval {
    pub const NAI: Interval = Interval {
        min: f32::NAN,
        max: f32::NAN,
    };
    pub const INFINITE: Interval = Interval {
        min: f32::NEG_INFINITY,
        max: f32::INFINITY,
    };

    /// `Interval.of`; panics (like Java's IllegalArgumentException) on `max < min` or NaN bounds.
    pub fn of(min: f32, max: f32) -> Interval {
        if max < min {
            panic!("max ({max}) < min ({min})");
        } else if min.is_nan() || max.is_nan() {
            panic!("Bounds cannot include NaN [{min}; {max}]: use Interval.NaI explicitly");
        }
        Interval { min, max }
    }
    pub fn of_symmetric(range: f32) -> Interval {
        Self::of(-range, range)
    }
    pub fn of_exact(value: f32) -> Interval {
        Self::of(value, value)
    }
    pub fn min(&self) -> f32 {
        self.min
    }
    pub fn max(&self) -> f32 {
        self.max
    }
    pub fn is_nai(&self) -> bool {
        self.min.is_nan()
    }
    pub fn contains(&self, value: f32) -> bool {
        value >= self.min && value <= self.max
    }
    pub fn intersects(&self, other: &Interval) -> bool {
        self.min <= other.max && self.max >= other.min
    }

    /// `Interval.encapsulating(List<Interval>)`.
    pub fn encapsulating(intervals: &[Interval]) -> Interval {
        assert!(!intervals.is_empty(), "At least one interval required");
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for i in intervals {
            if !i.is_nai() {
                min = jmin(i.min, min);
                max = jmax(i.max, max);
            }
        }
        if max < min {
            Interval::NAI
        } else {
            Interval::of(min, max)
        }
    }
    /// `Interval.encapsulating(float, float)`.
    pub fn encapsulating_f(first: f32, second: f32) -> Interval {
        if first.is_nan() && second.is_nan() {
            Interval::NAI
        } else if first.is_nan() {
            Interval::of_exact(second)
        } else if second.is_nan() {
            Interval::of_exact(first)
        } else {
            Interval::of(jmin(first, second), jmax(first, second))
        }
    }
    fn encapsulating_if(first: Interval, second: f32) -> Interval {
        if second.is_nan() {
            first
        } else if first.is_nai() {
            Interval::of_exact(second)
        } else {
            Interval::of(jmin(first.min, second), jmax(first.max, second))
        }
    }

    pub fn add(left: Interval, right: Interval) -> Interval {
        let min = left.min + right.min;
        let max = left.max + right.max;
        if !min.is_nan() && !max.is_nan() {
            Interval::of(min, max)
        } else {
            Interval::NAI
        }
    }
    pub fn sub(left: Interval, right: Interval) -> Interval {
        let min = left.min - right.max;
        let max = left.max - right.min;
        if !min.is_nan() && !max.is_nan() {
            Interval::of(min, max)
        } else {
            Interval::NAI
        }
    }
    pub fn mul(left: Interval, right: Interval) -> Interval {
        if left.is_nai() || right.is_nai() {
            return Interval::NAI;
        }
        let min_min = mul_bound(left.min, right.min);
        let min_max = mul_bound(left.min, right.max);
        let max_min = mul_bound(left.max, right.min);
        let max_max = mul_bound(left.max, right.max);
        Interval::of(
            jmin(jmin(min_min, min_max), jmin(max_min, max_max)),
            jmax(jmax(min_min, min_max), jmax(max_min, max_max)),
        )
    }
    pub fn reciprocal(input: Interval) -> Interval {
        if !input.is_nai() && (input.min != 0.0 || input.max != 0.0) {
            if !input.contains(0.0) {
                Interval::of(1.0 / input.max, 1.0 / input.min)
            } else if input.max == 0.0 {
                Interval::of(f32::NEG_INFINITY, 1.0 / input.min)
            } else if input.min == 0.0 {
                Interval::of(1.0 / input.max, f32::INFINITY)
            } else {
                Interval::INFINITE
            }
        } else {
            Interval::NAI
        }
    }
    pub fn div(left: Interval, right: Interval) -> Interval {
        Self::mul(left, Self::reciprocal(right))
    }
    pub fn min_of(left: Interval, right: Interval) -> Interval {
        if !left.is_nai() && !right.is_nai() {
            Interval::of(jmin(left.min, right.min), jmin(left.max, right.max))
        } else {
            Interval::NAI
        }
    }
    pub fn max_of(left: Interval, right: Interval) -> Interval {
        if !left.is_nai() && !right.is_nai() {
            Interval::of(jmax(left.min, right.min), jmax(left.max, right.max))
        } else {
            Interval::NAI
        }
    }
    pub fn clamp(input: Interval, min: f32, max: f32) -> Interval {
        assert!(min <= max, "min ({min}) > max ({max})");
        if input.is_nai() {
            Interval::NAI
        } else if input.min >= max {
            Interval::of(max, max)
        } else if input.max <= min {
            Interval::of(min, min)
        } else {
            Interval::of(jmax(input.min, min), jmin(input.max, max))
        }
    }
    pub fn abs(input: Interval) -> Interval {
        if input.is_nai() {
            return Interval::NAI;
        }
        let max = jmax(input.min.abs(), input.max.abs());
        if input.contains(0.0) {
            Interval::of(0.0, max)
        } else {
            Interval::of(jmin(input.min.abs(), input.max.abs()), max)
        }
    }
    pub fn square(input: Interval) -> Interval {
        if input.is_nai() {
            return Interval::NAI;
        }
        let max = jmax(mth::square(input.min), mth::square(input.max));
        if input.contains(0.0) {
            Interval::of(0.0, max)
        } else {
            Interval::of(jmin(mth::square(input.min), mth::square(input.max)), max)
        }
    }

    pub fn pow(base: Interval, exponent: Interval) -> Interval {
        if base.is_nai() || exponent.is_nai() {
            return Interval::NAI;
        }
        if base.min == base.max {
            return pow_scalar(base.min, exponent);
        }
        let mut result = Interval::encapsulating(&[
            pow_scalar(base.min, exponent),
            pow_scalar(base.max, exponent),
        ]);
        if base.contains(0.0) {
            if base.max > 0.0 {
                result = Interval::encapsulating(&[result, pow_scalar(0.0, exponent)]);
            }
            if base.min < 0.0 {
                result = Interval::encapsulating(&[result, pow_scalar(-0.0, exponent)]);
            }
        }
        result
    }

    pub fn log(input: Interval) -> Interval {
        if input.max < 0.0 {
            Interval::NAI
        } else {
            let clipped = Interval::max_of(input, Interval::of_exact(0.0));
            Interval::map_monotonic(clipped, |x| (x as f64).ln() as f32)
        }
    }

    /// `Interval.mapMonotonic`; panics if the operator yields NaN (like Java's IllegalStateException).
    pub fn map_monotonic(input: Interval, op: impl Fn(f32) -> f32) -> Interval {
        if input.is_nai() {
            return Interval::NAI;
        }
        let mapped_min = op(input.min);
        let mapped_max = op(input.max);
        if !mapped_min.is_nan() && !mapped_max.is_nan() {
            Interval::of(jmin(mapped_min, mapped_max), jmax(mapped_min, mapped_max))
        } else {
            panic!("Monotonic operator should not produce NaN")
        }
    }

    pub fn lerp(alpha: Interval, first: Interval, second: Interval) -> Interval {
        if alpha.is_nai() || first.is_nai() || second.is_nai() {
            return Interval::NAI;
        }
        Interval::encapsulating(&[
            lerp_f(alpha, first.min, second.min),
            lerp_f(alpha, first.max, second.min),
            lerp_f(alpha, first.min, second.max),
            lerp_f(alpha, first.max, second.max),
        ])
    }

    pub fn sign(input: Interval) -> Interval {
        if input.is_nai() {
            Interval::NAI
        } else if input.min == input.max {
            Interval::of_exact(signum(input.min))
        } else if input.contains(0.0) {
            if input.min == 0.0 {
                Interval::of(0.0, 1.0)
            } else if input.max == 0.0 {
                Interval::of(-1.0, 0.0)
            } else {
                Interval::of(-1.0, 1.0)
            }
        } else {
            Interval::of_exact(if input.min > 0.0 { 1.0 } else { -1.0 })
        }
    }
}

impl Eq for Interval {}
impl Hash for Interval {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if self.is_nai() {
            0u8.hash(state);
        } else {
            self.min.to_bits().hash(state);
            self.max.to_bits().hash(state);
        }
    }
}

fn mul_bound(left: f32, right: f32) -> f32 {
    if left != 0.0 && right != 0.0 {
        left * right
    } else {
        0.0
    }
}

fn java_pow_f(base: f32, exponent: f32) -> f32 {
    java_pow(base as f64, exponent as f64) as f32
}

/// `Math.pow(double, double)`. Rust's `powf` delegates to the C library `pow` which is correctly rounded
/// in practice for the argument ranges worldgen uses; the special cases below follow the Java spec where
/// the C99 spec differs (`pow(1, NaN)`, `pow(-1, +-inf)`).
pub fn java_pow(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() {
        return f64::NAN;
    }
    if exponent == 0.0 {
        return 1.0;
    }
    if base.is_nan() {
        return f64::NAN;
    }
    if base.abs() == 1.0 && exponent.is_infinite() {
        return f64::NAN;
    }
    base.powf(exponent)
}

fn pow_scalar(base: f32, exponent: Interval) -> Interval {
    if base.is_nan() || exponent.is_nai() {
        return Interval::NAI;
    }
    if exponent.min == exponent.max {
        let value = java_pow_f(base, exponent.min);
        return if value.is_nan() {
            Interval::NAI
        } else {
            Interval::of_exact(value)
        };
    }
    if base == 0.0 {
        return Interval::mul(
            pow_zero_base(exponent),
            Interval::of_exact(1.0f32.copysign(base)),
        );
    }
    if base == 1.0 {
        return Interval::of_exact(1.0);
    }
    if base > 0.0 {
        pow_positive_base(base, exponent)
    } else {
        pow_negative_base(base, exponent)
    }
}

fn pow_positive_base(base: f32, exponent: Interval) -> Interval {
    if exponent.min.is_finite() && exponent.max.is_finite() {
        Interval::encapsulating_f(
            java_pow_f(base, exponent.min),
            java_pow_f(base, exponent.max),
        )
    } else {
        pow_infinite_exponent(base, exponent)
    }
}

fn pow_zero_base(exponent: Interval) -> Interval {
    if exponent.contains(0.0) {
        if exponent.max == 0.0 {
            Interval::of(1.0, f32::INFINITY)
        } else if exponent.min == 0.0 {
            Interval::of(0.0, 1.0)
        } else {
            Interval::of(0.0, f32::INFINITY)
        }
    } else if exponent.max < 0.0 {
        Interval::of_exact(f32::INFINITY)
    } else {
        Interval::of_exact(0.0)
    }
}

fn pow_infinite_exponent(base: f32, exponent: Interval) -> Interval {
    if exponent.min.is_infinite() && exponent.max.is_infinite() {
        Interval::of(0.0, f32::INFINITY)
    } else if exponent.min.is_infinite() {
        if base < 1.0 {
            Interval::of(java_pow_f(base, exponent.max), f32::INFINITY)
        } else {
            Interval::of(0.0, java_pow_f(base, exponent.max))
        }
    } else if base < 1.0 {
        Interval::of(0.0, java_pow_f(base, exponent.min))
    } else {
        Interval::of(java_pow_f(base, exponent.min), f32::INFINITY)
    }
}

fn pow_negative_base(base: f32, exponent: Interval) -> Interval {
    let exponent_min_int = exponent.min.ceil();
    let exponent_max_int = exponent.max.floor();
    if exponent_max_int < exponent_min_int {
        return Interval::NAI;
    }
    let base_to_min_int = java_pow_f(base, exponent_min_int);
    let base_to_max_int = java_pow_f(base, exponent_max_int);
    let mut result = Interval::encapsulating_f(base_to_min_int, base_to_max_int);
    if exponent_min_int.is_infinite() {
        result = Interval::encapsulating_if(result, -base_to_min_int);
    } else if exponent_min_int + 1.0 < exponent_max_int {
        result = Interval::encapsulating_if(result, java_pow_f(base, exponent_min_int + 1.0));
    }
    if exponent_max_int.is_infinite() {
        result = Interval::encapsulating_if(result, -base_to_max_int);
    } else if exponent_max_int - 1.0 > exponent_min_int {
        result = Interval::encapsulating_if(result, java_pow_f(base, exponent_max_int - 1.0));
    }
    result
}

fn lerp_f(alpha: Interval, first: f32, second: f32) -> Interval {
    if alpha.is_nai() || first.is_nan() || second.is_nan() {
        Interval::NAI
    } else if first.is_finite() && second.is_finite() {
        Interval::encapsulating_f(
            lerp_finite_bound(alpha.min, first, second),
            lerp_finite_bound(alpha.max, first, second),
        )
    } else {
        lerp_infinite_bounds(alpha, first, second)
    }
}

fn lerp_finite_bound(alpha: f32, first: f32, second: f32) -> f32 {
    first + mul_bound(alpha, second - first)
}

fn lerp_infinite_bounds(alpha: Interval, first: f32, second: f32) -> Interval {
    if first == second {
        return Interval::of_exact(first);
    }
    let new_min = lerp_infinite_bound(alpha.min, first, second);
    let new_max = lerp_infinite_bound(alpha.max, first, second);
    if !new_min.is_nan() && !new_max.is_nan() {
        Interval::encapsulating_f(new_min, new_max)
    } else {
        Interval::NAI
    }
}

fn lerp_infinite_bound(alpha: f32, first: f32, second: f32) -> f32 {
    let first_part = mul_bound(1.0 - alpha, first);
    let second_part = mul_bound(alpha, second);
    if !first_part.is_infinite() || !second_part.is_infinite() {
        first_part + second_part
    } else if alpha <= 0.0 {
        if second > first {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        }
    } else if alpha >= 1.0 {
        if second > first {
            f32::INFINITY
        } else {
            f32::NEG_INFINITY
        }
    } else {
        f32::NAN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_float_math() {
        assert!(jmin(f32::NAN, 1.0).is_nan());
        assert!(jmax(1.0, f32::NAN).is_nan());
        assert!(jmin(0.0, -0.0).is_sign_negative());
        assert!(jmax(-0.0, 0.0).is_sign_positive());
        assert_eq!(round_f32(-0.5), 0.0);
        assert_eq!(round_f32(2.5), 3.0);
        assert_eq!(round_f32(-2.5), -2.0);
        assert_eq!(floor_div(-7, 2), -4);
        assert_eq!(floor_mod(-7, 2), 1);
        assert_eq!(mth::round_toward(17, 16), 32);
    }

    #[test]
    fn interval_basics() {
        let a = Interval::of(-1.0, 2.0);
        let b = Interval::of(3.0, 4.0);
        assert_eq!(Interval::mul(a, b), Interval::of(-4.0, 8.0));
        assert_eq!(Interval::sub(a, b), Interval::of(-5.0, -1.0));
        assert!(Interval::reciprocal(a) == Interval::INFINITE);
        assert_eq!(Interval::abs(a), Interval::of(0.0, 2.0));
        assert_eq!(Interval::clamp(a, 0.0, 1.0), Interval::of(0.0, 1.0));
    }
}
