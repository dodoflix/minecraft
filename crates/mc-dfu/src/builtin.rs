//! The primitive codecs of `Codec` (`Codec.BOOL`, `Codec.INT`, ...), `PASSTHROUGH` and the `Codec.*Range` helpers.

use crate::codec::{Codec, Dynamic};
use crate::result::DataResult;
use mc_core::math::java::{java_double_to_string, java_float_to_string};
use std::cmp::Ordering;
use std::sync::LazyLock;

/// `Float.compare`: `-0.0 < 0.0`, `NaN` equals itself and is greater than everything.
pub fn java_float_cmp(a: f32, b: f32) -> Ordering {
    let canon = |v: f32| if v.is_nan() { f32::NAN } else { v };
    canon(a).total_cmp(&canon(b))
}

/// `Double.compare`.
pub fn java_double_cmp(a: f64, b: f64) -> Ordering {
    let canon = |v: f64| if v.is_nan() { f64::NAN } else { v };
    canon(a).total_cmp(&canon(b))
}

macro_rules! prim {
    ($name:ident, $ty:ty, $get:ident, $make:ident) => {
        pub static $name: LazyLock<Codec<$ty>> = LazyLock::new(|| {
            Codec::primitive(|ops, input| ops.get_number_value(input).map(|n| n.$get()), |ops, v| ops.$make(*v))
        });
    };
}

prim!(BYTE, i8, byte_value, create_byte);
prim!(SHORT, i16, short_value, create_short);
prim!(INT, i32, int_value, create_int);
prim!(LONG, i64, long_value, create_long);
prim!(FLOAT, f32, float_value, create_float);
prim!(DOUBLE, f64, double_value, create_double);

pub static BOOL: LazyLock<Codec<bool>> =
    LazyLock::new(|| Codec::primitive(|ops, input| ops.get_boolean_value(input), |ops, v| ops.create_boolean(*v)));

pub static STRING: LazyLock<Codec<String>> =
    LazyLock::new(|| Codec::primitive(|ops, input| ops.get_string_value(input), |ops, v| ops.create_string(v.clone())));

/// `Codec.BYTE_BUFFER` (as `Vec<i8>`).
pub static BYTE_BUFFER: LazyLock<Codec<Vec<i8>>> =
    LazyLock::new(|| Codec::primitive(|ops, input| ops.get_byte_buffer(input), |ops, v| ops.create_byte_list(v.clone())));

/// `Codec.INT_STREAM` (as `Vec<i32>`).
pub static INT_STREAM: LazyLock<Codec<Vec<i32>>> =
    LazyLock::new(|| Codec::primitive(|ops, input| ops.get_int_stream(input), |ops, v| ops.create_int_list(v.clone())));

/// `Codec.LONG_STREAM` (as `Vec<i64>`).
pub static LONG_STREAM: LazyLock<Codec<Vec<i64>>> =
    LazyLock::new(|| Codec::primitive(|ops, input| ops.get_long_stream(input), |ops, v| ops.create_long_list(v.clone())));

/// `Codec.PASSTHROUGH`.
pub static PASSTHROUGH: LazyLock<Codec<Dynamic>> = LazyLock::new(|| {
    Codec::from_fns(
        |ops, input: &Dynamic, prefix| {
            if input.value.is_empty_value() {
                return DataResult::success(prefix);
            }
            let casted = input.convert(ops);
            if prefix.is_empty_value() {
                return DataResult::success(casted);
            }
            // merge into a map, else into a list
            let to_map = ops.get_map(&casted).flat_map(|m| ops.merge_to_map(&prefix, m.entries()));
            if let DataResult::Success(r) = to_map {
                return DataResult::success(r);
            }
            let to_list = ops.get_list(&casted).flat_map(|l| ops.merge_to_list(&prefix, l.into_owned()));
            if let DataResult::Success(r) = to_list {
                return DataResult::success(r);
            }
            let (p, c) = (prefix.clone(), casted);
            DataResult::error_partial(move || format!("Don't know how to merge {p} and {c}"), prefix)
        },
        |ops, input| DataResult::success((Dynamic::new(*ops, input.clone()), ops.empty())),
    )
});

/// `Codec.unit(())` / vanilla's `Unit.CODEC`: an empty map.
pub static UNIT: LazyLock<Codec<()>> = LazyLock::new(|| Codec::unit(()));

fn range_error<T: std::fmt::Display + Send + Sync + 'static>(v: T, min: String, max: String) -> DataResult<T> {
    DataResult::error(move || format!("Value {v} outside of range [{min}:{max}]"))
}

/// `Codec.intRange(min, max)`.
pub fn int_range(min: i32, max: i32) -> Codec<i32> {
    INT.flat_xmap(
        move |v| if v >= min && v <= max { DataResult::success(v) } else { range_error(v, min.to_string(), max.to_string()) },
        move |v| if *v >= min && *v <= max { DataResult::success(*v) } else { range_error(*v, min.to_string(), max.to_string()) },
    )
}

/// `Codec.floatRange(min, max)`.
pub fn float_range(min: f32, max: f32) -> Codec<f32> {
    let check = move |v: f32| {
        if java_float_cmp(v, min) != Ordering::Less && java_float_cmp(v, max) != Ordering::Greater {
            DataResult::success(v)
        } else {
            let (s, lo, hi) = (java_float_to_string(v), java_float_to_string(min), java_float_to_string(max));
            DataResult::error(move || format!("Value {s} outside of range [{lo}:{hi}]"))
        }
    };
    FLOAT.flat_xmap(check, move |v| check(*v))
}

/// `Codec.doubleRange(min, max)`.
pub fn double_range(min: f64, max: f64) -> Codec<f64> {
    let check = move |v: f64| {
        if java_double_cmp(v, min) != Ordering::Less && java_double_cmp(v, max) != Ordering::Greater {
            DataResult::success(v)
        } else {
            let (s, lo, hi) = (java_double_to_string(v), java_double_to_string(min), java_double_to_string(max));
            DataResult::error(move || format!("Value {s} outside of range [{lo}:{hi}]"))
        }
    };
    DOUBLE.flat_xmap(check, move |v| check(*v))
}

/// UTF-16 length like Java `String.length()`.
pub fn java_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `Codec.string(min, max)`.
pub fn string_range(min: usize, max: usize) -> Codec<String> {
    STRING.validate(move |value| {
        let length = java_len(&value);
        if length < min {
            DataResult::error(move || format!("String \"{value}\" is too short: {length}, expected range [{min}-{max}]"))
        } else if length > max {
            DataResult::error(move || format!("String \"{value}\" is too long: {length}, expected range [{min}-{max}]"))
        } else {
            DataResult::success(value)
        }
    })
}

/// `Codec.sizeLimitedString(max)`.
pub fn size_limited_string(max: usize) -> Codec<String> {
    string_range(0, max)
}

/// `Codec.stringResolver(toString, fromString)`.
pub fn string_resolver<E: 'static>(
    to_string: impl Fn(&E) -> Option<String> + Send + Sync + 'static,
    from_string: impl Fn(&str) -> Option<E> + Send + Sync + 'static,
) -> Codec<E> {
    STRING.flat_xmap(
        move |name| match from_string(&name) {
            Some(e) => DataResult::success(e),
            None => DataResult::error(move || format!("Unknown element name:{name}")),
        },
        move |e| match to_string(e) {
            Some(s) => DataResult::success(s),
            None => DataResult::error_str("Element with unknown name"),
        },
    )
}

/// `StringRepresentable.fromEnum(values)` codec: the serialized name, "Unknown element name:x" on failure.
pub fn string_enum_codec<E: Copy + PartialEq + Send + Sync + 'static>(values: &'static [(E, &'static str)]) -> Codec<E> {
    string_resolver(
        move |e| values.iter().find(|(v, _)| v == e).map(|(_, n)| n.to_string()),
        move |name| values.iter().find(|(_, n)| *n == name).map(|(v, _)| *v),
    )
}
