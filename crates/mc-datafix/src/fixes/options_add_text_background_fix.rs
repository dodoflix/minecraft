//! `OptionsAddTextBackgroundFix`: derives `textBackgroundOpacity` from `chatOpacity`.

use crate::datafix::DataFix;
use crate::fixes::references::References;
use crate::schema::Schema;
use mc_core::math::java::{self, java_double_to_string};

/// `new OptionsAddTextBackgroundFix(outputSchema, changesType)`.
pub fn new(output_schema: &Schema, changes_type: bool) -> DataFix {
    DataFix::new(
        "OptionsAddTextBackgroundFix",
        output_schema,
        changes_type,
        |df| {
            df.fix_type_everywhere_typed(
                "OptionsAddTextBackgroundFix",
                &df.input_schema().get_type(&References::OPTIONS),
                |input| {
                    input.update_remainder(|tag| {
                        let fixed = tag
                            .get("chatOpacity")
                            .as_string()
                            .map(|value| {
                                let opacity = calculate_background(value.as_lossy_str());
                                tag.set(
                                    "textBackgroundOpacity",
                                    tag.create_string(java_double_to_string(opacity)),
                                )
                            })
                            .result();
                        fixed.unwrap_or(tag)
                    })
                },
            )
        },
    )
}

/// `calculateBackground(textOpacity)`: 0.5 when the text is not a double.
fn calculate_background(text_opacity: &str) -> f64 {
    match java_parse_double(text_opacity) {
        Some(value) => {
            let text_alpha = 0.9 * value + 0.1;
            text_alpha / 2.0
        }
        None => 0.5,
    }
}

/// `Double.parseDouble(s)`: `None` where Java throws `NumberFormatException`. Java trims characters up to U+0020,
/// accepts a trailing `f`/`F`/`d`/`D`, only the exact words `NaN` and `Infinity`, and hexadecimal floats.
// shortcut: a hexadecimal float with more than 53 significant bits rounds twice, upgrade when such a value must be exact.
fn java_parse_double(s: &str) -> Option<f64> {
    let s = s.trim_matches(|c: char| c <= ' ');
    let (negative, unsigned) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let magnitude = match unsigned {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        _ => match unsigned
            .strip_prefix("0x")
            .or_else(|| unsigned.strip_prefix("0X"))
        {
            Some(hex) => parse_hex_double(hex)?,
            None => parse_decimal_double(unsigned)?,
        },
    };
    Some(if negative { -magnitude } else { magnitude })
}

fn parse_decimal_double(s: &str) -> Option<f64> {
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

fn parse_hex_double(s: &str) -> Option<f64> {
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
    if value == 0.0 {
        return Some(0.0);
    }
    Some(value * java::pow(2.0, f64::from(exponent.clamp(-4000, 4000) as i32)))
}
