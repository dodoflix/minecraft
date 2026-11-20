//! Java library semantics that differ from Rust's std: `Math.min/max/round/floorDiv/floorMod/rint`,
//! `String.hashCode`, `Integer.parseInt`.

/// `Math.min(float, float)`: NaN-propagating, `-0.0 < 0.0`.
pub fn min_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.is_sign_negative() {
        return b;
    }
    if a <= b { a } else { b }
}

/// `Math.max(float, float)`: NaN-propagating, `-0.0 < 0.0`.
pub fn max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        return b;
    }
    if a >= b { a } else { b }
}

/// `Math.min(double, double)`: NaN-propagating, `-0.0 < 0.0`.
pub fn min_f64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.is_sign_negative() {
        return b;
    }
    if a <= b { a } else { b }
}

/// `Math.max(double, double)`: NaN-propagating, `-0.0 < 0.0`.
pub fn max_f64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        return b;
    }
    if a >= b { a } else { b }
}

/// `Math.floorDiv(int, int)` (wraps on `MIN / -1`; panics on zero divisor like `ArithmeticException`).
pub fn floor_div_i32(a: i32, b: i32) -> i32 {
    let q = a.wrapping_div(b);
    if (a ^ b) < 0 && q.wrapping_mul(b) != a {
        q - 1
    } else {
        q
    }
}

/// `Math.floorDiv(long, long)`.
pub fn floor_div_i64(a: i64, b: i64) -> i64 {
    let q = a.wrapping_div(b);
    if (a ^ b) < 0 && q.wrapping_mul(b) != a {
        q - 1
    } else {
        q
    }
}

/// `Math.floorMod(int, int)`.
pub fn floor_mod_i32(a: i32, b: i32) -> i32 {
    let m = a.wrapping_rem(b);
    if (m ^ b) < 0 && m != 0 { m + b } else { m }
}

/// `Math.floorMod(long, long)`.
pub fn floor_mod_i64(a: i64, b: i64) -> i64 {
    let m = a.wrapping_rem(b);
    if (m ^ b) < 0 && m != 0 { m + b } else { m }
}

/// `Math.round(double)`: `floor(x + 0.5)` computed exactly, saturating; NaN -> 0.
pub fn round_f64(x: f64) -> i64 {
    let f = x.floor();
    (if x - f >= 0.5 { f + 1.0 } else { f }) as i64
}

/// `Math.round(float)`: `floor(x + 0.5)` computed exactly, saturating; NaN -> 0.
pub fn round_f32(x: f32) -> i32 {
    let f = x.floor();
    (if x - f >= 0.5 { f + 1.0 } else { f }) as i32
}

/// `Math.rint(double)`: round half to even.
pub fn rint(x: f64) -> f64 {
    x.round_ties_even()
}

/// `String.hashCode()` over UTF-16 code units.
pub fn java_string_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// First code point of every block of ten decimal digits of the basic multilingual plane (`Character.digit(char, 10)`
/// reads UTF-16 units, so supplementary digits are not digits); the digit value is the offset into the block.
const DECIMAL_DIGIT_ZEROS: [u32; 37] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
];

/// `Character.digit(c, 10)`: `None` for -1.
fn java_digit10(c: char) -> Option<u32> {
    let c = c as u32;
    let block = DECIMAL_DIGIT_ZEROS
        .partition_point(|&zero| zero <= c)
        .checked_sub(1)?;
    (c - DECIMAL_DIGIT_ZEROS[block] < 10).then(|| c - DECIMAL_DIGIT_ZEROS[block])
}

/// `Integer.parseInt(s)`: an optional sign and decimal digits of any script (`None` where Java throws
/// `NumberFormatException`).
pub fn parse_int(s: &str) -> Option<i32> {
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for c in digits.chars() {
        value = value * 10 + i64::from(java_digit10(c)?);
        if value > i64::from(i32::MAX) + 1 {
            return None;
        }
    }
    i32::try_from(if negative { -value } else { value }).ok()
}

/// Lays out Rust `{:e}`-style `digits[.digits]e<exp>` (sign already removed) the way `Double/Float.toString` does.
fn java_fp_layout(neg: bool, sci: &str) -> String {
    let (mant, exp) = sci.split_once('e').expect("`{:e}` output has an exponent");
    let exp: i32 = exp.parse().expect("`{:e}` exponent is an integer");
    // Trailing zeros (from a two-digit candidate like `5.0e-1`) are not significant.
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let digits = match digits.trim_end_matches('0') {
        "" => "0",
        t => t,
    }
    .to_string();
    let mut out = String::with_capacity(digits.len() + 8);
    if neg {
        out.push('-');
    }
    if (-3..7).contains(&exp) {
        if exp < 0 {
            out.push_str("0.");
            out.extend(std::iter::repeat_n('0', (-exp - 1) as usize));
            out.push_str(&digits);
        } else {
            let int_len = exp as usize + 1;
            if digits.len() <= int_len {
                out.push_str(&digits);
                out.extend(std::iter::repeat_n('0', int_len - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        }
    } else {
        out.push_str(&digits[..1]);
        out.push('.');
        out.push_str(if digits.len() > 1 { &digits[1..] } else { "0" });
        out.push('E');
        out.push_str(&exp.to_string());
    }
    out
}

macro_rules! java_fp_to_string {
    ($x:expr, $t:ty) => {{
        let x: $t = $x;
        if x.is_nan() {
            return "NaN".to_string();
        }
        if x.is_infinite() {
            return if x < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
        }
        if x == 0.0 {
            return if x.is_sign_negative() { "-0.0" } else { "0.0" }.to_string();
        }
        let neg = x < 0.0;
        let a = x.abs();
        let mut sci = format!("{:e}", a);
        // JDK >= 19 picks, among the shortest round-tripping decimals, the one closest to x (ties: even digit);
        // Rust's shortest generator breaks exact ties upward. When the shortest has one digit, the candidates are
        // the two-digit decimals instead (Double.MIN_VALUE -> 4.9E-324, not 5E-324). The exactly rounded
        // `{:.Ne}` rendering is that closest candidate whenever it round-trips.
        let m = sci.split_once('e').map_or(1, |(m, _)| m.chars().filter(|c| c.is_ascii_digit()).count());
        let n = if m == 1 { 1 } else { m - 1 };
        let exact = format!("{:.*e}", n, a);
        if exact.parse::<$t>() == Ok(a) {
            sci = exact;
        }
        java_fp_layout(neg, &sci)
    }};
}

/// `Double.toString(x)` (JDK 19+ shortest-decimal specification).
pub fn java_double_to_string(x: f64) -> String {
    java_fp_to_string!(x, f64)
}

/// `Float.toString(x)` (JDK 19+ shortest-decimal specification).
pub fn java_float_to_string(x: f32) -> String {
    java_fp_to_string!(x, f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_div_mod() {
        assert_eq!(floor_div_i32(-7, 2), -4);
        assert_eq!(floor_div_i32(7, -2), -4);
        assert_eq!(floor_div_i32(i32::MIN, -1), i32::MIN);
        assert_eq!(floor_mod_i32(-7, 3), 2);
        assert_eq!(floor_mod_i32(7, -3), -2);
        assert_eq!(floor_mod_i64(-1, 16), 15);
        assert_eq!(floor_div_i64(-1, 16), -1);
    }

    #[test]
    fn round_and_hash() {
        assert_eq!(round_f64(-2.5), -2);
        assert_eq!(round_f64(2.5), 3);
        assert_eq!(round_f32(-0.5), 0);
        assert_eq!(java_string_hash("hello"), 99162322);
        assert_eq!(java_string_hash(""), 0);
    }

    #[test]
    fn parse_int_follows_integer_parse_int() {
        for (text, want) in [
            ("12", Some(12)),
            ("+5", Some(5)),
            ("-2147483648", Some(i32::MIN)),
            ("2147483647", Some(i32::MAX)),
            ("2147483648", None),
            ("-2147483649", None),
            ("-0", Some(0)),
            ("\u{663}", Some(3)),
            ("\u{ff11}\u{ff12}", Some(12)),
            ("\u{1d7f7}7", None),
            ("", None),
            ("-", None),
            ("+", None),
            (" 1", None),
            ("1 ", None),
            ("0x1", None),
            ("1_0", None),
            ("--1", None),
        ] {
            assert_eq!(parse_int(text), want, "{text:?}");
        }
    }
}
