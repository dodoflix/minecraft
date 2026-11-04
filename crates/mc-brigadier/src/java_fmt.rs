//! `Double.toString` / `Float.toString` (JDK 19+ shortest-repr algorithm), needed to reproduce
//! brigadier's "Double must not be less than 1.0, found 0.5" messages byte for byte.

fn render(neg: bool, digits: &str, exp10: i32) -> String {
    // value = 0.d1d2d3... * 10^(exp10+1) i.e. d1.d2d3... * 10^exp10
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if (-3..7).contains(&exp10) {
        if exp10 >= 0 {
            let int_len = exp10 as usize + 1;
            if digits.len() <= int_len {
                out.push_str(digits);
                out.extend(std::iter::repeat_n('0', int_len - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        } else {
            out.push_str("0.");
            out.extend(std::iter::repeat_n('0', (-exp10 - 1) as usize));
            out.push_str(digits);
        }
    } else {
        out.push_str(&digits[..1]);
        out.push('.');
        out.push_str(if digits.len() > 1 { &digits[1..] } else { "0" });
        out.push('E');
        out.push_str(&exp10.to_string());
    }
    out
}

/// Splits Rust's `{:e}` output ("d.ddde-5") into (digits, exponent).
fn split_sci(s: &str) -> (String, i32) {
    let (mant, exp) = s.split_once('e').expect("scientific format");
    (mant.replace('.', ""), exp.parse().expect("exponent"))
}

macro_rules! java_to_string {
    ($name:ident, $t:ty) => {
        pub fn $name(v: $t) -> String {
            if v.is_nan() {
                return "NaN".into();
            }
            if v.is_infinite() {
                return if v < 0.0 { "-Infinity" } else { "Infinity" }.into();
            }
            if v == 0.0 {
                return if v.is_sign_negative() { "-0.0" } else { "0.0" }.into();
            }
            let neg = v < 0.0;
            let a = v.abs();
            let (mut digits, mut exp) = split_sci(&format!("{:e}", a));
            // Java picks, among the shortest round-tripping decimals, the one closest to the exact
            // value (ties to even) and prints at least two digits; `{:.N e}` is exactly that rounding.
            let n = digits.len().max(2);
            let (d2, e2) = split_sci(&format!("{:.*e}", n - 1, a));
            let back: $t = format!("{}e{}", d2, e2 - (n as i32 - 1)).parse().unwrap_or(0.0);
            if back == a {
                digits = d2;
                exp = e2;
            }
            let digits = digits.trim_end_matches('0');
            render(neg, if digits.is_empty() { "0" } else { digits }, exp)
        }
    };
}

java_to_string!(double_to_string, f64);
java_to_string!(float_to_string, f32);
