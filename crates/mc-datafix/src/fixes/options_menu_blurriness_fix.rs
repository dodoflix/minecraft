//! `OptionsMenuBlurrinessFix`: the menu background blurriness option becomes an integer.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::math::java::{self, round_f32};

/// `new OptionsMenuBlurrinessFix(outputSchema)`.
pub fn new(output_schema: &Schema) -> DataFix {
    DataFix::new("OptionsMenuBlurrinessFix", output_schema, false, |df| {
        df.fix_type_everywhere_typed(
            "OptionsMenuBlurrinessFix",
            &df.input_schema().get_type(&References::OPTIONS),
            |input| {
                input.update_remainder(|tag| {
                    tag.update("menuBackgroundBlurriness", |value| {
                        let int_value =
                            convert_to_int_range(value.as_string_or("0.5").as_lossy_str());
                        value.create_string(int_value.to_string())
                    })
                })
            },
        )
    })
}

/// `Math.round(Float.parseFloat(blurriness) * 10.0F)`, 5 when the text is not a float.
fn convert_to_int_range(float_blurriness: &str) -> i32 {
    match java_parse_float(float_blurriness) {
        Some(value) => round_f32(value * 10.0),
        None => 5,
    }
}

/// `Float.parseFloat(s)`: `None` where Java throws `NumberFormatException`. Java trims characters up to U+0020,
/// accepts a trailing `f`/`F`/`d`/`D`, only the exact words `NaN` and `Infinity`, and hexadecimal floats.
// shortcut: a hexadecimal float with more than 53 significant bits rounds twice, upgrade when such a value must be exact.
fn java_parse_float(s: &str) -> Option<f32> {
    let s = s.trim_matches(|c: char| c <= ' ');
    let (negative, unsigned) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let magnitude = match unsigned {
        "NaN" => f32::NAN,
        "Infinity" => f32::INFINITY,
        _ => match unsigned
            .strip_prefix("0x")
            .or_else(|| unsigned.strip_prefix("0X"))
        {
            Some(hex) => parse_hex_float(hex)?,
            None => parse_decimal_float(unsigned)?,
        },
    };
    Some(if negative { -magnitude } else { magnitude })
}

fn parse_decimal_float(s: &str) -> Option<f32> {
    let s = s.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(s);
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(at) => (&s[..at], Some(&s[at + 1..])),
        None => (s, None),
    };
    let (int_part, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if !digits(int_part) || !digits(fraction) || int_part.len() + fraction.len() == 0 {
        return None;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !digits(exponent) {
            return None;
        }
    }
    s.parse().ok()
}

fn parse_hex_float(s: &str) -> Option<f32> {
    let s = s.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(s);
    let (mantissa, exponent) = s.split_once(['p', 'P'])?;
    let (int_part, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let hex = |part: &str| part.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex(int_part) || !hex(fraction) || int_part.len() + fraction.len() == 0 {
        return None;
    }
    let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut exponent: i64 = exponent.parse().unwrap_or(if exponent.starts_with('-') {
        i64::MIN / 2
    } else {
        i64::MAX / 2
    });
    let mut value = 0f64;
    for c in int_part.chars() {
        value = value * 16.0 + f64::from(c.to_digit(16)?);
    }
    for c in fraction.chars() {
        value = value * 16.0 + f64::from(c.to_digit(16)?);
        exponent -= 4;
    }
    let scaled = value * java::pow(2.0, f64::from(exponent.clamp(-4000, 4000) as i32));
    Some(scaled as f32)
}
