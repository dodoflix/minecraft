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

/// `ExtraCodecs.optionalAlwaysPresentFieldOf(element, name, default)`: absent reads as `default`, always written.
pub fn optional_always_present_field_of<A: Clone + Send + Sync + 'static>(element: &Codec<A>, name: &str, default: A) -> crate::MapCodec<A> {
    element.optional_field_of(name).xmap(move |o| o.unwrap_or_else(|| default.clone()), |a: &A| Some(a.clone()))
}

/// `ExtraCodecs.optionalEmptyMap(codec)`: an empty map reads as `None` and `None` writes an empty map.
pub fn optional_empty_map<A: 'static>(codec: &Codec<A>) -> Codec<Option<A>> {
    let (e, d) = (codec.clone(), codec.clone());
    Codec::from_fns(
        move |ops, input: &Option<A>, prefix| match input {
            None => DataResult::success(ops.empty_map()),
            Some(a) => e.encode(ops, a, prefix),
        },
        move |ops, input| {
            if let DataResult::Success(m) = ops.get_map(input) {
                if m.entries().is_empty() {
                    return DataResult::success((None, input.clone()));
                }
            }
            d.decode(ops, input).map(|(a, r)| (Some(a), r))
        },
    )
}

/// `Codec.dispatchedMap(keyCodec, valueCodecFn)`: a map whose value codec depends on the key (key/value pairs in
/// input order; duplicate keys are errors).
pub fn dispatched_map<K: PartialEq + 'static, V: 'static>(
    key_codec: &Codec<K>,
    value_codec: impl Fn(&K) -> DataResult<Codec<V>> + Send + Sync + 'static,
) -> Codec<Vec<(K, V)>> {
    let (kc1, kc2) = (key_codec.clone(), key_codec.clone());
    let value_codec = std::sync::Arc::new(value_codec);
    let vc2 = value_codec.clone();
    Codec::from_fns(
        move |ops, input: &Vec<(K, V)>, prefix| {
            let mut b = ops.map_builder();
            for (k, v) in input {
                let encoded = value_codec(k).flat_map(|c| c.encode_start(ops, v));
                b.add_results(kc1.encode_start(ops, k), encoded);
            }
            b.build(&prefix)
        },
        move |ops, input| {
            let map = match ops.get_map(input) {
                DataResult::Success(m) => m,
                DataResult::Error { message, .. } => return DataResult::error_msg(message, None),
            };
            let mut entries: Vec<(K, V)> = Vec::new();
            let mut failed: Vec<(Dyn, Dyn)> = Vec::new();
            let mut result: DataResult<()> = DataResult::success(());
            for (k, v) in map.entries() {
                let key_result = kc2.parse(ops, k);
                let value_result = match &key_result {
                    DataResult::Success(key) => vc2(key).flat_map(|c| c.parse(ops, v)),
                    DataResult::Error { message, partial } => match partial.as_ref().map(|p| vc2(p)) {
                        Some(r) => DataResult::error_msg(message.clone(), None).apply2(r.flat_map(|c| c.parse(ops, v)), |_: (), v| v),
                        None => DataResult::error_msg(message.clone(), None),
                    },
                };
                let entry_result = key_result.apply2(value_result, |k, v| (k, v));
                let step: DataResult<()> = match entry_result.error_msg_ref() {
                    Some(m) => DataResult::error_msg(m.clone(), None),
                    None => DataResult::success(()),
                };
                let is_error = entry_result.is_error();
                if let Some((key, val)) = entry_result.result_or_partial() {
                    if entries.iter().any(|(ek, _)| *ek == key) {
                        failed.push((k.clone(), v.clone()));
                        let key_text = k.clone();
                        result = result.apply2(DataResult::error(move || format!("Duplicate entry for key: '{key_text}'")), |_, _: ()| ());
                        continue;
                    }
                    entries.push((key, val));
                }
                if is_error {
                    failed.push((k.clone(), v.clone()));
                }
                result = result.apply2(step, |_, _| ());
            }
            let errors = ops.create_map(failed);
            let pair = (entries, input.clone());
            match result {
                DataResult::Success(()) => DataResult::success(pair),
                DataResult::Error { message, .. } => DataResult::error_msg(
                    crate::Msg::new(move || format!("{} missed input: {}", message.get(), errors)),
                    Some(pair),
                ),
            }
        },
    )
}

use crate::Dyn;

/// `Identifier.CODEC`.
pub static IDENTIFIER: LazyLock<Codec<mc_core::ident::Identifier>> = LazyLock::new(|| {
    STRING.comap_flat_map(
        |s| match mc_core::ident::Identifier::read(&s) {
            Ok(id) => DataResult::success(id),
            Err(e) => DataResult::error(move || e.clone()),
        },
        |id| id.to_string(),
    )
});
