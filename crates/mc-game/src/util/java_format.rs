//! `String.format(Locale.ROOT, "%.<n>f", double)`: Java rounds the shortest decimal representation half-up
//! (`FormattedFloatingDecimal`), which differs from C's exact-binary rounding (`2.675` -> `2.68`).
//!
//! shortcut: digits come from the shortest repr (`Double.toString`); the JDK's legacy `FloatingDecimal` digit
//! generator can differ in the 17th digit for a few values, which only matters at exact rounding ties.

use mc_core::math::java::java_double_to_string;

/// `%.{precision}f`.
pub fn format_fixed(value: f64, precision: usize) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    let negative = value.is_sign_negative();
    let sign = if negative { "-" } else { "" };
    if value.is_infinite() {
        return format!("{sign}Infinity");
    }
    // value = 0.D1D2D3... * 10^dec_exp
    let repr = java_double_to_string(value.abs());
    let (mantissa, exp) = repr.split_once('E').map_or((repr.as_str(), 0i32), |(m, e)| (m, e.parse().unwrap_or(0)));
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits: Vec<u8> = int_part.bytes().chain(frac_part.bytes()).map(|b| b - b'0').collect();
    let mut dec_exp = int_part.len() as i32 + exp;
    let leading = digits.iter().take_while(|d| **d == 0).count();
    digits.drain(..leading);
    dec_exp -= leading as i32;
    if digits.is_empty() {
        return format!("{sign}{}", zero(precision));
    }
    // keep `dec_exp + precision` significant digits, rounding half up
    let keep = dec_exp + precision as i32;
    if keep < 0 {
        return format!("{sign}{}", zero(precision));
    }
    let keep = keep as usize;
    if keep < digits.len() {
        let round_up = digits[keep] >= 5;
        digits.truncate(keep);
        if round_up {
            let mut i = keep;
            loop {
                if i == 0 {
                    digits.insert(0, 1);
                    dec_exp += 1;
                    break;
                }
                i -= 1;
                if digits[i] == 9 {
                    digits[i] = 0;
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        }
    }
    // render
    let mut int_digits = String::new();
    let mut frac_digits = String::new();
    if dec_exp <= 0 {
        int_digits.push('0');
        for _ in 0..(-dec_exp) {
            frac_digits.push('0');
        }
        frac_digits.extend(digits.iter().map(|d| (b'0' + d) as char));
    } else {
        for (i, d) in digits.iter().enumerate() {
            if (i as i32) < dec_exp {
                int_digits.push((b'0' + d) as char);
            } else {
                frac_digits.push((b'0' + d) as char);
            }
        }
        while (int_digits.len() as i32) < dec_exp {
            int_digits.push('0');
        }
    }
    frac_digits.truncate(precision);
    while frac_digits.len() < precision {
        frac_digits.push('0');
    }
    if precision == 0 { format!("{sign}{int_digits}") } else { format!("{sign}{int_digits}.{frac_digits}") }
}

fn zero(precision: usize) -> String {
    if precision == 0 { "0".to_string() } else { format!("0.{}", "0".repeat(precision)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_java_half_up() {
        // Values checked against `String.format(Locale.ROOT, "%.2f", x)` semantics.
        assert_eq!(format_fixed(0.125, 2), "0.13");
        assert_eq!(format_fixed(2.675, 2), "2.68");
        assert_eq!(format_fixed(50.0, 2), "50.00");
        assert_eq!(format_fixed(0.0, 2), "0.00");
        assert_eq!(format_fixed(0.004, 2), "0.00");
        assert_eq!(format_fixed(0.005, 2), "0.01");
        assert_eq!(format_fixed(9.995, 2), "10.00");
        assert_eq!(format_fixed(99.999, 2), "100.00");
        assert_eq!(format_fixed(1234567.891, 2), "1234567.89");
        assert_eq!(format_fixed(1.0e-5, 2), "0.00");
        assert_eq!(format_fixed(1.5e10, 2), "15000000000.00");
        assert_eq!(format_fixed(-1.234, 2), "-1.23");
        assert_eq!(format_fixed(f64::NAN, 2), "NaN");
    }
}
