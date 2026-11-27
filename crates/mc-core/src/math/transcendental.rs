//! `Math.sin`, `Math.cos`, `Math.log` and `Math.pow` of the JDK, independent of the platform's C library.
//!
//! The JVM computes these with its own routines and is correctly rounded for all but a tiny fraction of the inputs. Measured
//! over 2^21 inputs per function on Java 25 HotSpot (Temurin 25.0.5.1), these functions differ from it in `sin` 0.160% of the
//! inputs, `cos` 0.064%, `pow` 0.034%, `log` 3 inputs; the C library's `sin`, `cos`, `log` and `pow` differ more often
//! (0.205%, 0.095%, 0.107%, 131 inputs) and change between libc versions. These functions evaluate in double-double
//! arithmetic (about 100 bits) and round once, so the result is the correctly rounded value except for inputs whose exact
//! result lies within 2^-90 of a rounding boundary. `pow` of exact results (integer powers with at most 128 significant
//! bits) is rounded from the exact value, so ties go to even.
//!
//! shortcut: the rare hard-to-round `pow` inputs (`0.9999999999999999^1.5`) are not re-evaluated at higher precision;
//! upgrade when a vanilla scenario depends on one.
//! `Math.log1p` is `StrictMath.log1p`: the fdlibm algorithm.
use num_bigint::BigUint;
use std::sync::LazyLock;

/// An unevaluated sum `hi + lo` with `|lo| <= ulp(hi) / 2`.
#[derive(Clone, Copy)]
struct Dd {
    hi: f64,
    lo: f64,
}

fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    Dd {
        hi: s,
        lo: (a - (s - bb)) + (b - bb),
    }
}

/// `two_sum` for `|a| >= |b|`.
fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd {
        hi: s,
        lo: b - (s - a),
    }
}

impl Dd {
    const ZERO: Dd = Dd { hi: 0.0, lo: 0.0 };

    fn new(hi: f64) -> Dd {
        Dd { hi, lo: 0.0 }
    }

    fn neg(self) -> Dd {
        Dd {
            hi: -self.hi,
            lo: -self.lo,
        }
    }

    fn add(self, o: Dd) -> Dd {
        let s = two_sum(self.hi, o.hi);
        let t = two_sum(self.lo, o.lo);
        let u = quick_two_sum(s.hi, s.lo + t.hi);
        quick_two_sum(u.hi, u.lo + t.lo)
    }

    fn sub(self, o: Dd) -> Dd {
        self.add(o.neg())
    }

    fn add_f(self, b: f64) -> Dd {
        let s = two_sum(self.hi, b);
        quick_two_sum(s.hi, s.lo + self.lo)
    }

    fn mul(self, o: Dd) -> Dd {
        let p = self.hi * o.hi;
        let e = self.hi.mul_add(o.hi, -p);
        quick_two_sum(p, e + (self.hi * o.lo + self.lo * o.hi))
    }

    fn mul_f(self, b: f64) -> Dd {
        let p = self.hi * b;
        let e = self.hi.mul_add(b, -p);
        quick_two_sum(p, e + self.lo * b)
    }

    fn div(self, o: Dd) -> Dd {
        let q1 = self.hi / o.hi;
        let r = self.sub(o.mul_f(q1));
        let q2 = r.hi / o.hi;
        let r = r.sub(o.mul_f(q2));
        let q3 = r.hi / o.hi;
        quick_two_sum(q1, q2).add_f(q3)
    }

    fn scale(self, f: f64) -> Dd {
        Dd {
            hi: self.hi * f,
            lo: self.lo * f,
        }
    }
}

/// Fraction bits of the big fixed-point constants.
const P: u64 = 1500;
/// Fraction bits of `2/pi` for the reduction of arguments above `FAST_LIMIT`.
const F: u64 = 1400;
/// Largest `|x|` reduced with the six-chunk `pi/2` (the multiples `n * chunk` stay exact).
const FAST_LIMIT: f64 = 524_288.0;
const SIN_TERMS: usize = 16;
const COS_TERMS: usize = 17;
const LN_TERMS: usize = 23;
const EXP_TERMS: usize = 11;

struct Consts {
    /// `pi/2` in six chunks of 30 bits and the tail below them.
    pio2: [f64; 6],
    pio2_tail: Dd,
    /// `ln 2` in three chunks of 40 bits, the tail below them and the whole value.
    ln2: [f64; 3],
    ln2_tail: Dd,
    ln2_dd: Dd,
    /// `(-1)^k / (2k+1)!` and `(-1)^k / (2k)!`.
    sin_c: [Dd; SIN_TERMS],
    cos_c: [Dd; COS_TERMS],
    /// `1 / (2k+1)`.
    inv_odd: [Dd; LN_TERMS],
    /// `1 / j!`.
    inv_fact: [Dd; EXP_TERMS],
    pio2_fix: BigUint,
    inv_2pi: BigUint,
}

fn big(v: u64) -> BigUint {
    BigUint::from(v)
}

/// `sum_k (+-1)^k x^(2k+1)/(2k+1)` with `x = 1/n`, in `P` fraction bits (`alternate`: arctan, else atanh).
fn series(n: u64, alternate: bool) -> BigUint {
    let mut power = (big(1) << P) / n;
    let mut sum = BigUint::from(0u32);
    let mut neg = BigUint::from(0u32);
    let mut k = 0u64;
    while power != BigUint::from(0u32) {
        let term = &power / (2 * k + 1);
        if alternate && k % 2 == 1 {
            neg += term;
        } else {
            sum += term;
        }
        power /= n * n;
        k += 1;
    }
    sum - neg
}

/// The value of `n / 2^scale` as a double-double (truncated at about 106 bits).
fn to_dd(n: &BigUint, scale: u64) -> Dd {
    fn top(n: &BigUint, scale: u64) -> (f64, BigUint) {
        let bits = n.bits();
        if bits == 0 {
            return (0.0, BigUint::from(0u32));
        }
        let shift = bits.saturating_sub(53);
        let lead = (n >> shift).iter_u64_digits().next().unwrap_or(0);
        let value = libm::ldexp(lead as f64, shift as i32 - scale as i32);
        (value, n - (big(lead) << shift))
    }
    let (hi, rest) = top(n, scale);
    let (lo, _) = top(&rest, scale);
    quick_two_sum(hi, lo)
}

fn chunk(fixed: &BigUint, bits: u64, index: u64) -> f64 {
    let mask = (big(1) << bits) - big(1);
    let part = (fixed >> (P - bits * (index + 1))) & mask;
    to_dd(&part, bits * (index + 1)).hi
}

static CONSTS: LazyLock<Consts> = LazyLock::new(|| {
    let pi = (series(5, true) * 16u32) - (series(239, true) * 4u32);
    let pio2_fix = &pi >> 1u32;
    let ln2_fix = series(3, false) * 2u32;
    // pio2 chunk 0 keeps the integer bit: 31 bits
    let mut pio2 = [0.0; 6];
    for (i, c) in pio2.iter_mut().enumerate() {
        let part = if i == 0 {
            &pio2_fix >> (P - 30)
        } else {
            (&pio2_fix >> (P - 30 * (i as u64 + 1))) & ((big(1) << 30u32) - big(1))
        };
        *c = to_dd(&part, 30 * (i as u64 + 1)).hi;
    }
    let pio2_tail = to_dd(&(&pio2_fix & ((big(1) << (P - 180)) - big(1))), P);
    let ln2 = [0, 1, 2].map(|i| chunk(&ln2_fix, 40, i));
    let ln2_tail = to_dd(&(&ln2_fix & ((big(1) << (P - 120)) - big(1))), P);
    let ln2_dd = to_dd(&ln2_fix, P);

    // 1/j! by repeated division: 40 terms cover the sine and cosine series
    let mut inv_fact = [Dd::ZERO; 40];
    inv_fact[0] = Dd::new(1.0);
    for j in 1..40 {
        inv_fact[j] = inv_fact[j - 1].div(Dd::new(j as f64));
    }
    let sign = |k: usize, d: Dd| if k % 2 == 1 { d.neg() } else { d };
    let sin_c = std::array::from_fn(|k| sign(k, inv_fact[2 * k + 1]));
    let cos_c = std::array::from_fn(|k| sign(k, inv_fact[2 * k]));
    let inv_odd = std::array::from_fn(|k| Dd::new(1.0).div(Dd::new((2 * k + 1) as f64)));
    let inv_fact_exp = std::array::from_fn(|j| inv_fact[j]);
    let inv_2pi = (big(1) << (F + 1 + P)) / &pi;
    Consts {
        pio2,
        pio2_tail,
        ln2,
        ln2_tail,
        ln2_dd,
        sin_c,
        cos_c,
        inv_odd,
        inv_fact: inv_fact_exp,
        pio2_fix,
        inv_2pi,
    }
});

/// `x mod pi/2` for `x >= 0`: the quadrant count `n mod 4` and the remainder `r` with `|r| <= pi/4`.
fn reduce(x: f64) -> (u32, Dd) {
    let c = &*CONSTS;
    if x < FAST_LIMIT {
        let n = (x * (2.0 / std::f64::consts::PI)).round();
        let mut r = Dd::new(x);
        for chunk in c.pio2 {
            r = r.add_f(-(n * chunk));
        }
        r = r.sub(c.pio2_tail.mul_f(n));
        return ((n as u64 & 3) as u32, r);
    }
    let bits = x.to_bits();
    let exp = (bits >> 52 & 0x7ff) as i64;
    let mant = bits & ((1 << 52) - 1);
    let (m, e) = if exp == 0 {
        (mant, -1074)
    } else {
        (mant | 1 << 52, exp - 1075)
    };
    // x * 2/pi = m * inv_2pi * 2^(e - F)
    let product = big(m) * &c.inv_2pi;
    let s = (F as i64 - e) as u64;
    let whole = &product >> s;
    let mut frac = product - (&whole << s);
    let mut n = (&whole & big(3)).iter_u64_digits().next().unwrap_or(0) as u32;
    let mut negative = false;
    if frac >= (big(1) << (s - 1)) {
        n = (n + 1) & 3;
        frac = (big(1) << s) - frac;
        negative = true;
    }
    let r = to_dd(&(frac * &c.pio2_fix), s + P);
    (n, if negative { r.neg() } else { r })
}

/// `sin(r)` for `|r| <= 0.8`.
fn sin_kernel(r: Dd) -> Dd {
    let c = &*CONSTS;
    let z = r.mul(r);
    let mut p = c.sin_c[SIN_TERMS - 1];
    for k in (0..SIN_TERMS - 1).rev() {
        p = p.mul(z).add(c.sin_c[k]);
    }
    r.mul(p)
}

/// `cos(r)` for `|r| <= 0.8`.
fn cos_kernel(r: Dd) -> Dd {
    let c = &*CONSTS;
    let z = r.mul(r);
    let mut p = c.cos_c[COS_TERMS - 1];
    for k in (0..COS_TERMS - 1).rev() {
        p = p.mul(z).add(c.cos_c[k]);
    }
    p
}

/// `Math.sin`.
pub fn sin(x: f64) -> f64 {
    let a = x.abs();
    if a < 1.0 / 134_217_728.0 {
        return x; // |sin x - x| < 2^-56 |x|; also keeps -0.0 and the subnormals
    }
    if !a.is_finite() {
        return f64::NAN;
    }
    let (q, r) = reduce(a);
    let v = match q {
        0 => sin_kernel(r),
        1 => cos_kernel(r),
        2 => sin_kernel(r).neg(),
        _ => cos_kernel(r).neg(),
    };
    if x < 0.0 { -v.hi } else { v.hi }
}

/// `Math.cos`.
pub fn cos(x: f64) -> f64 {
    let a = x.abs();
    if a < 1.0 / 134_217_728.0 {
        return 1.0;
    }
    if !a.is_finite() {
        return f64::NAN;
    }
    let (q, r) = reduce(a);
    match q {
        0 => cos_kernel(r),
        1 => sin_kernel(r).neg(),
        2 => cos_kernel(r).neg(),
        _ => sin_kernel(r),
    }
    .hi
}

/// `ln x` for finite `x > 0`.
fn ln_dd(x: f64) -> Dd {
    let c = &*CONSTS;
    let (x, mut k) = if x < f64::MIN_POSITIVE {
        (x * 18_014_398_509_481_984.0, -54) // 2^54
    } else {
        (x, 0)
    };
    let bits = x.to_bits();
    k += (bits >> 52) as i32 - 1023;
    let mut m = f64::from_bits(bits & ((1 << 52) - 1) | 1023 << 52);
    if m > std::f64::consts::SQRT_2 {
        m *= 0.5;
        k += 1;
    }
    // ln m = 2 atanh(z), z = (m - 1) / (m + 1)
    let z = Dd::new(m - 1.0).div(two_sum(m, 1.0));
    let z2 = z.mul(z);
    let mut p = c.inv_odd[LN_TERMS - 1];
    for j in (0..LN_TERMS - 1).rev() {
        p = p.mul(z2).add(c.inv_odd[j]);
    }
    c.ln2_dd.mul_f(f64::from(k)).add(z.mul(p).scale(2.0))
}

/// `e^y = v * 2^n` with `v` in about `[0.70, 1.42]`; `|y| < 800`.
fn exp_dd(y: Dd) -> (Dd, i32) {
    let c = &*CONSTS;
    let n = (y.hi * std::f64::consts::LOG2_E).round();
    let mut r = y;
    for chunk in c.ln2 {
        r = r.add_f(-(n * chunk));
    }
    r = r.sub(c.ln2_tail.mul_f(n)).scale(1.0 / 256.0);
    // e^r - 1 = r * sum_{j>=1} r^(j-1)/j!, then eight squarings of (1 + s)
    let mut p = c.inv_fact[EXP_TERMS - 1];
    for j in (1..EXP_TERMS - 1).rev() {
        p = p.mul(r).add(c.inv_fact[j]);
    }
    let mut s = r.mul(p);
    for _ in 0..8 {
        s = s.scale(2.0).add(s.mul(s));
    }
    (s.add_f(1.0), n as i32)
}

/// `Math.log`.
pub fn log(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return x;
    }
    ln_dd(x).hi
}

fn with_high(x: f64, high: i32) -> f64 {
    f64::from_bits((x.to_bits() & 0xffff_ffff) | u64::from(high as u32) << 32)
}

/// `Math.log1p` (`StrictMath.log1p`: the original fdlibm algorithm, which differs from the FreeBSD/musl rewrite in the
/// small-argument cutoff and the handling of `f` near zero).
pub fn log1p(x: f64) -> f64 {
    const LN2_HI: f64 = 0.693_147_180_369_123_8;
    const LN2_LO: f64 = 1.908_214_929_270_587_7e-10;
    const TWO54: f64 = 1.801_439_850_948_198_4e16;
    const LP: [f64; 7] = [
        0.666_666_666_666_673_5,
        0.399_999_999_994_094_2,
        0.285_714_287_436_623_9,
        0.222_221_984_321_497_84,
        0.181_835_721_616_180_5,
        0.153_138_376_992_093_73,
        0.147_981_986_051_165_86,
    ];
    let hx = (x.to_bits() >> 32) as u32 as i32;
    let ax = hx & 0x7fff_ffff;
    let mut k = 1;
    let mut f = 0.0;
    let mut hu = 0;
    let mut c = 0.0;
    if hx < 0x3fda_827a {
        if ax >= 0x3ff0_0000 {
            return if x == -1.0 {
                f64::NEG_INFINITY
            } else {
                f64::NAN
            };
        }
        if ax < 0x3e20_0000 {
            return if TWO54 + x > 0.0 && ax < 0x3c90_0000 {
                x
            } else {
                x - x * x * 0.5
            };
        }
        if hx > 0 || hx <= 0xbfd2_bec3_u32 as i32 {
            k = 0;
            f = x;
            hu = 1;
        }
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    if k != 0 {
        let u;
        if hx < 0x4340_0000 {
            u = 1.0 + x;
            hu = (u.to_bits() >> 32) as u32 as i32;
            k = (hu >> 20) - 1023;
            c = if k > 0 { 1.0 - (u - x) } else { x - (u - 1.0) };
            c /= u;
        } else {
            u = x;
            hu = (u.to_bits() >> 32) as u32 as i32;
            k = (hu >> 20) - 1023;
        }
        hu &= 0x000f_ffff;
        let m = if hu < 0x6a09e {
            with_high(u, hu | 0x3ff0_0000)
        } else {
            k += 1;
            let m = with_high(u, hu | 0x3fe0_0000);
            hu = (0x0010_0000 - hu) >> 2;
            m
        };
        f = m - 1.0;
    }
    let hfsq = 0.5 * f * f;
    let kf = f64::from(k);
    if hu == 0 {
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                c += kf * LN2_LO;
                kf * LN2_HI + c
            };
        }
        let r = hfsq * (1.0 - 0.666_666_666_666_666_6 * f);
        return if k == 0 {
            f - r
        } else {
            kf * LN2_HI - ((r - (kf * LN2_LO + c)) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let r = z
        * (LP[0] + z * (LP[1] + z * (LP[2] + z * (LP[3] + z * (LP[4] + z * (LP[5] + z * LP[6]))))));
    if k == 0 {
        f - (hfsq - s * (hfsq + r))
    } else {
        kf * LN2_HI - ((hfsq - (s * (hfsq + r) + (kf * LN2_LO + c))) - f)
    }
}

/// 1 when `y` is an odd integer, 0 when it is an even integer, -1 otherwise.
fn integer_parity(y: f64) -> i32 {
    if y.abs() >= 9_007_199_254_740_992.0 {
        0
    } else if y != y.trunc() {
        -1
    } else {
        i32::from((y as i64) & 1 == 1)
    }
}

/// `v * 2^n` rounded once to the nearest double, also in the subnormal range (where rounding `v.hi` first would round twice).
fn scale_rounded(v: Dd, n: i32) -> f64 {
    let exponent = (v.hi.to_bits() >> 52 & 0x7ff) as i32 - 1023 + n;
    if exponent >= -1022 {
        return libm::scalbn(v.hi, n);
    }
    // in units of the smallest subnormal the sum is below 2^53: `hi` is exact and `lo` only decides the rounding
    let shift = n + 1074;
    let t = libm::scalbn(v.hi, shift);
    let whole = t.floor();
    let mut fraction = (t - whole) + libm::scalbn(v.lo, shift);
    let carry = fraction.floor();
    let mut k = whole + carry;
    fraction -= carry;
    if fraction > 0.5 || (fraction == 0.5 && k % 2.0 != 0.0) {
        k += 1.0;
    }
    libm::scalbn(k, -1074)
}

/// `x^n` for a positive integer `n >= 2` when it is exactly an integer below 2^128 times a power of two, correctly rounded
/// (ties to even, subnormals included); `None` when the exact value does not fit. The double-double evaluation cannot tell
/// an exact tie (`53.5^8`) from a value a hair above or below it.
fn exact_integer_power(x: f64, y: f64) -> Option<f64> {
    if y < 2.0 || y > f64::from(1u32 << 20) || y != y.trunc() {
        return None;
    }
    let n = y as u32;
    let bits = x.to_bits();
    let biased = (bits >> 52 & 0x7ff) as i32;
    let fraction = bits & 0xf_ffff_ffff_ffff;
    let (mantissa, exponent) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | 1 << 52, biased - 1075)
    };
    let shift = mantissa.trailing_zeros();
    let odd = u128::from(mantissa >> shift);
    let exponent = i64::from(exponent + shift as i32) * i64::from(n);
    let mut power = 1u128;
    for _ in 0..n {
        if odd == 1 {
            break;
        }
        power = power.checked_mul(odd)?;
    }
    Some(round_scaled(power, exponent))
}

/// `m * 2^e` rounded to the nearest double (ties to even); `m > 0`.
fn round_scaled(m: u128, e: i64) -> f64 {
    let length = i64::from(128 - m.leading_zeros());
    let top = length - 1 + e;
    if top > 1100 {
        return f64::INFINITY;
    }
    let quantum = (top - 52).max(-1074);
    let drop = quantum - e;
    let mantissa = if drop <= 0 {
        m << (-drop) as u32
    } else if drop > 128 {
        0
    } else {
        let kept = if drop == 128 { 0 } else { m >> drop };
        let rest = if drop == 128 {
            m
        } else {
            m & ((1u128 << drop) - 1)
        };
        let half = 1u128 << (drop - 1);
        kept + u128::from(rest > half || (rest == half && kept & 1 == 1))
    };
    libm::scalbn(mantissa as f64, quantum as i32)
}

/// `Math.pow`.
pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if y.is_infinite() {
        let ax = x.abs();
        return if ax == 1.0 {
            f64::NAN
        } else if (ax > 1.0) == (y > 0.0) {
            f64::INFINITY
        } else {
            0.0
        };
    }
    let parity = integer_parity(y);
    if x == 0.0 || x.is_infinite() {
        let odd_negative = x.is_sign_negative() && parity == 1;
        let magnitude_up = (x == 0.0) == (y < 0.0);
        let m = if magnitude_up { f64::INFINITY } else { 0.0 };
        return if odd_negative { -m } else { m };
    }
    if x < 0.0 && parity < 0 {
        return f64::NAN;
    }
    let ax = x.abs();
    let magnitude = if ax == 1.0 {
        1.0
    } else if y == -1.0 {
        1.0 / ax
    } else if y == 0.5 {
        ax.sqrt()
    } else if let Some(exact) = exact_integer_power(ax, y) {
        exact
    } else {
        let l = ln_dd(ax);
        let approx = l.hi * y;
        if approx > 709.8 {
            f64::INFINITY
        } else if approx < -745.2 {
            0.0
        } else {
            let (v, n) = exp_dd(l.mul_f(y));
            scale_rounded(v, n)
        }
    };
    if x < 0.0 && parity == 1 {
        -magnitude
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(sin(1.0), 0.841_470_984_807_896_5);
        assert_eq!(cos(1.0), 0.540_302_305_868_139_8);
        assert_eq!(sin(-0.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(log(2.0), std::f64::consts::LN_2);
        assert_eq!(log(1.0).to_bits(), 0);
        assert_eq!(log(5e-324), -744.440_071_921_381_2);
        assert_eq!(pow(2.0, 10.0), 1024.0);
        assert_eq!(pow(10.0, -2.0), 0.01);
        assert_eq!(pow(-8.0, 3.0), -512.0);
        assert!(pow(-8.0, 0.5).is_nan());
        assert!(pow(1.0, f64::INFINITY).is_nan());
        assert_eq!(pow(0.0, -3.0), f64::INFINITY);
        assert_eq!(pow(-0.0, -3.0), f64::NEG_INFINITY);
        assert_eq!(pow(1.0e200, 2.0), f64::INFINITY);
    }

    #[test]
    fn pow_rounds_exact_and_subnormal_results_once() {
        // 53.5^8 is exactly half way between two doubles: the even one
        assert_eq!(pow(53.5, 8.0).to_bits(), 0x42ce_8567_f888_ca30);
        assert_eq!(pow(6.0, 34.0).to_bits(), 0x456d_9fe7_7988_1944);
        // 1 / (0.5 - 2^-54) is just above a midpoint
        assert_eq!(
            pow(0.499_999_999_999_999_94, -1.0).to_bits(),
            0x4000_0000_0000_0001
        );
        // subnormal results come from the whole double-double value
        assert_eq!(
            pow(1.658_547_250_439_473_5e-16, 19.5).to_bits(),
            0x000d_d9c0_f2f2_6d8b
        );
        assert_eq!(pow(2.0, -1074.0).to_bits(), 1);
        assert_eq!(pow(2.0, -1075.0).to_bits(), 0);
        assert_eq!(pow(2.0, 1024.0), f64::INFINITY);
        assert_eq!(pow(2.0, 1023.0), 2f64.powi(1023));
    }
}
