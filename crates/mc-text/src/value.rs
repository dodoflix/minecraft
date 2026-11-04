//! Java `Number` modelling for translatable arguments (`ExtraCodecs.JAVA` values), including
//! `Float.toString` / `Double.toString` formatting.

use serde_json::Number;
use std::fmt;

/// A Java `Number` as produced by the DFU `JavaOps` conversion: NBT numeric tags keep their exact
/// width; JSON numbers are narrowed the way `JsonOps.convertTo` does (see [`Num::from_json`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Num {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
}

impl Num {
    /// `JsonOps.convertTo(<other ops>, JsonPrimitive number)`: integral values become the smallest of
    /// byte/short/int/long, other values a float when exactly representable, else a double.
    pub fn from_json(n: &Number) -> Num {
        let exact_long = match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(l), _, _) => Some(l),
            (None, Some(_), _) => None,
            (None, None, Some(f)) if f.fract() == 0.0 && (-(2f64.powi(63))..2f64.powi(63)).contains(&f) => Some(f as i64),
            _ => None,
        };
        match exact_long {
            Some(l) if l as i8 as i64 == l => Num::Byte(l as i8),
            Some(l) if l as i16 as i64 == l => Num::Short(l as i16),
            Some(l) if l as i32 as i64 == l => Num::Int(l as i32),
            Some(l) => Num::Long(l),
            None => {
                let d = n.as_f64().unwrap_or(0.0);
                if d as f32 as f64 == d { Num::Float(d as f32) } else { Num::Double(d) }
            }
        }
    }

    /// `Number.intValue()`.
    pub fn int_value(&self) -> i32 {
        match self {
            Num::Byte(v) => *v as i32,
            Num::Short(v) => *v as i32,
            Num::Int(v) => *v,
            Num::Long(v) => *v as i32,
            Num::Float(v) => *v as i32,
            Num::Double(v) => *v as i32,
        }
    }

    /// `Number.doubleValue()`.
    pub fn double_value(&self) -> f64 {
        match self {
            Num::Byte(v) => *v as f64,
            Num::Short(v) => *v as f64,
            Num::Int(v) => *v as f64,
            Num::Long(v) => *v as f64,
            Num::Float(v) => *v as f64,
            Num::Double(v) => *v,
        }
    }

    /// The JSON number for this value (`JsonPrimitive(Number)`); `None` for NaN/infinity.
    pub fn to_json_number(&self) -> Option<Number> {
        Some(match self {
            Num::Byte(v) => Number::from(*v),
            Num::Short(v) => Number::from(*v),
            Num::Int(v) => Number::from(*v),
            Num::Long(v) => Number::from(*v),
            // Gson prints `Float.toString`; parse that so `0.1f` becomes 0.1 and not 0.10000000149011612.
            Num::Float(v) => Number::from_f64(java_float_to_string(*v).parse::<f64>().ok()?)?,
            Num::Double(v) => Number::from_f64(*v)?,
        })
    }
}

pub(crate) fn big_decimal_low_bits(f: f64) -> i128 {
    let t = f.trunc();
    if !t.is_finite() || t.abs() >= 2f64.powi(100) {
        // Past 2^100 a double's low 32 bits are all zero.
        return 0;
    }
    t as i128
}

/// `Object.toString()` of the Java number.
impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Num::Byte(v) => write!(f, "{v}"),
            Num::Short(v) => write!(f, "{v}"),
            Num::Int(v) => write!(f, "{v}"),
            Num::Long(v) => write!(f, "{v}"),
            Num::Float(v) => f.write_str(&java_float_to_string(*v)),
            Num::Double(v) => f.write_str(&java_double_to_string(*v)),
        }
    }
}

/// `Double.toString(double)` (JDK 19+ shortest-repr algorithm).
pub fn java_double_to_string(v: f64) -> String {
    java_fp_to_string(v.is_nan(), v.is_infinite(), v.is_sign_negative(), v == 0.0, v.abs() < 1e-3 || v.abs() >= 1e7, &format!("{:e}", v.abs()))
}

/// `Float.toString(float)`.
pub fn java_float_to_string(v: f32) -> String {
    java_fp_to_string(v.is_nan(), v.is_infinite(), v.is_sign_negative(), v == 0.0, v.abs() < 1e-3 || v.abs() >= 1e7, &format!("{:e}", v.abs()))
}

/// `sci` is Rust's shortest round-trip scientific form of `|v|` (`1.2345e-5`).
fn java_fp_to_string(nan: bool, inf: bool, neg: bool, zero: bool, use_sci: bool, sci: &str) -> String {
    if nan {
        return "NaN".into();
    }
    let sign = if neg { "-" } else { "" };
    if inf {
        return format!("{sign}Infinity");
    }
    if zero {
        return format!("{sign}0.0");
    }
    let (mant, exp) = sci.split_once('e').unwrap_or((sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    if use_sci {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        format!("{sign}{first}.{rest}E{exp}")
    } else if exp >= 0 {
        let int_len = exp as usize + 1;
        if digits.len() <= int_len {
            format!("{sign}{digits}{}.0", "0".repeat(int_len - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..int_len], &digits[int_len..])
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_float_formatting() {
        assert_eq!(java_double_to_string(1.0), "1.0");
        assert_eq!(java_double_to_string(1.5e10), "1.5E10");
        assert_eq!(java_double_to_string(0.001), "0.001");
        assert_eq!(java_double_to_string(0.0001), "1.0E-4");
        assert_eq!(java_double_to_string(123456.789), "123456.789");
        assert_eq!(java_double_to_string(1e7), "1.0E7");
        assert_eq!(java_double_to_string(-0.0), "-0.0");
        assert_eq!(java_float_to_string(0.1), "0.1");
        assert_eq!(java_float_to_string(16777216.0), "1.6777216E7");
    }
}
