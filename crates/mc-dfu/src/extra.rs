//! `net.minecraft.util.ExtraCodecs` and other small shared codec helpers.

use crate::builtin::*;
use crate::codec::{Codec, Either};
use crate::result::DataResult;
use std::cmp::Ordering;
use std::sync::LazyLock;

fn int_with_message(min: i32, max: i32, msg: impl Fn(i32) -> String + Send + Sync + Clone + 'static) -> Codec<i32> {
    INT.validate(move |v| if v >= min && v <= max { DataResult::success(v) } else { let m = msg(v); DataResult::error(move || m.clone()) })
}

fn long_with_message(min: i64, max: i64, msg: impl Fn(i64) -> String + Send + Sync + Clone + 'static) -> Codec<i64> {
    LONG.validate(move |v| if v >= min && v <= max { DataResult::success(v) } else { let m = msg(v); DataResult::error(move || m.clone()) })
}

pub static NON_NEGATIVE_INT: LazyLock<Codec<i32>> =
    LazyLock::new(|| int_with_message(0, i32::MAX, |n| format!("Value must be non-negative: {n}")));
pub static POSITIVE_INT: LazyLock<Codec<i32>> =
    LazyLock::new(|| int_with_message(1, i32::MAX, |n| format!("Value must be positive: {n}")));
pub static NON_NEGATIVE_LONG: LazyLock<Codec<i64>> =
    LazyLock::new(|| long_with_message(0, i64::MAX, |n| format!("Value must be non-negative: {n}")));
pub static POSITIVE_LONG: LazyLock<Codec<i64>> =
    LazyLock::new(|| long_with_message(1, i64::MAX, |n| format!("Value must be positive: {n}")));

/// `ExtraCodecs.NON_NEGATIVE_FLOAT` (`0.0 ..= Float.MAX_VALUE`, `Float.compareTo` ordering).
pub static NON_NEGATIVE_FLOAT: LazyLock<Codec<f32>> = LazyLock::new(|| {
    FLOAT.validate(|v| {
        if java_float_cmp(v, 0.0) != Ordering::Less && java_float_cmp(v, f32::MAX) != Ordering::Greater {
            DataResult::success(v)
        } else {
            let s = mc_core::math::java::java_float_to_string(v);
            DataResult::error(move || format!("Value must be non-negative: {s}"))
        }
    })
});
/// `ExtraCodecs.POSITIVE_FLOAT` (`> 0.0`).
pub static POSITIVE_FLOAT: LazyLock<Codec<f32>> = LazyLock::new(|| {
    FLOAT.validate(|v| {
        if java_float_cmp(v, 0.0) == Ordering::Greater && java_float_cmp(v, f32::MAX) != Ordering::Greater {
            DataResult::success(v)
        } else {
            let s = mc_core::math::java::java_float_to_string(v);
            DataResult::error(move || format!("Value must be positive: {s}"))
        }
    })
});

/// `ExtraCodecs.intRange(min, max)`.
pub fn extra_int_range(min: i32, max: i32) -> Codec<i32> {
    int_with_message(min, max, move |n| format!("Value must be within range [{min};{max}]: {n}"))
}

/// `ExtraCodecs.floatRange(min, max)`.
pub fn extra_float_range(min: f32, max: f32) -> Codec<f32> {
    FLOAT.validate(move |v| {
        if java_float_cmp(v, min) != Ordering::Less && java_float_cmp(v, max) != Ordering::Greater {
            DataResult::success(v)
        } else {
            let (s, lo, hi) = (
                mc_core::math::java::java_float_to_string(v),
                mc_core::math::java::java_float_to_string(min),
                mc_core::math::java::java_float_to_string(max),
            );
            DataResult::error(move || format!("Value must be within range [{lo};{hi}]: {s}"))
        }
    })
}

/// `ExtraCodecs.UNSIGNED_BYTE`.
pub static UNSIGNED_BYTE: LazyLock<Codec<i32>> = LazyLock::new(|| {
    BYTE.flat_comap_map(
        |b| (b as u8) as i32,
        |i: &i32| {
            if *i > 255 {
                let i = *i;
                DataResult::error(move || format!("Unsigned byte was too large: {i} > 255"))
            } else {
                DataResult::success(*i as i8)
            }
        },
    )
});

/// `ExtraCodecs.NON_EMPTY_STRING`.
pub static NON_EMPTY_STRING: LazyLock<Codec<String>> =
    LazyLock::new(|| STRING.validate(|v| if v.is_empty() { DataResult::error_str("Expected non-empty string") } else { DataResult::success(v) }));

/// `ExtraCodecs.nonEmptyList`.
pub fn non_empty_list<T: Clone + 'static>(list: &Codec<Vec<T>>) -> Codec<Vec<T>> {
    list.validate(|l| if l.is_empty() { DataResult::error_str("List must have contents") } else { DataResult::success(l) })
}

/// `ExtraCodecs.compactListCodec(element)`: a list, or a single element standing for a one-element list.
pub fn compact_list_codec<E: Clone + Send + Sync + 'static>(element: &Codec<E>) -> Codec<Vec<E>> {
    compact_list_codec_with(element, &element.list_of())
}

pub fn compact_list_codec_with<E: Clone + Send + Sync + 'static>(element: &Codec<E>, list: &Codec<Vec<E>>) -> Codec<Vec<E>> {
    list.either(element).xmap(
        |e| e.map(|l| l, |v| vec![v]),
        |v: &Vec<E>| if v.len() == 1 { Either::Right(v[0].clone()) } else { Either::Left(v.clone()) },
    )
}
