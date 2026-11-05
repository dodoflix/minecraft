//! `net.minecraft.advancements.predicates.MinMaxBounds` (`Ints`, `Doubles`, `FloatDegrees`).

use crate::codec::EncodeResult;
use crate::codec::{take, Decode, DecodeResult, Encode};
use mc_dfu::{java_double_cmp, java_float_cmp, Codec, DataResult, Either, DOUBLE, FLOAT, INT};
use std::cmp::Ordering;
use std::fmt::Debug;
use std::sync::LazyLock;

/// The numeric types of `MinMaxBounds<T extends Number & Comparable<T>>` with their Java `compareTo` / `equals`.
pub trait BoundNumber: Copy + Debug + Send + Sync + 'static {
    fn java_cmp(self, other: Self) -> Ordering;
    /// Java `toString` as printed inside `Optional[..]`.
    fn java_str(self) -> String;
}
impl BoundNumber for i32 {
    fn java_cmp(self, other: Self) -> Ordering {
        self.cmp(&other)
    }
    fn java_str(self) -> String {
        self.to_string()
    }
}
impl BoundNumber for i64 {
    fn java_cmp(self, other: Self) -> Ordering {
        self.cmp(&other)
    }
    fn java_str(self) -> String {
        self.to_string()
    }
}
impl BoundNumber for f32 {
    fn java_cmp(self, other: Self) -> Ordering {
        java_float_cmp(self, other)
    }
    fn java_str(self) -> String {
        mc_core::math::java::java_float_to_string(self)
    }
}
impl BoundNumber for f64 {
    fn java_cmp(self, other: Self) -> Ordering {
        java_double_cmp(self, other)
    }
    fn java_str(self) -> String {
        mc_core::math::java::java_double_to_string(self)
    }
}

fn opt_str<T: BoundNumber>(v: Option<T>) -> String {
    v.map_or("Optional.empty".into(), |v| format!("Optional[{}]", v.java_str()))
}

/// `MinMaxBounds.Bounds`.
#[derive(Clone, Copy, Debug)]
pub struct Bounds<T> {
    pub min: Option<T>,
    pub max: Option<T>,
}

impl<T: BoundNumber> PartialEq for Bounds<T> {
    fn eq(&self, other: &Self) -> bool {
        let eq = |a: Option<T>, b: Option<T>| match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a.java_cmp(b) == Ordering::Equal,
            _ => false,
        };
        eq(self.min, other.min) && eq(self.max, other.max)
    }
}

impl<T: BoundNumber> Bounds<T> {
    pub fn any() -> Self {
        Bounds { min: None, max: None }
    }
    pub fn exactly(value: T) -> Self {
        Bounds { min: Some(value), max: Some(value) }
    }
    pub fn between(min: T, max: T) -> Self {
        Bounds { min: Some(min), max: Some(max) }
    }
    pub fn at_least(value: T) -> Self {
        Bounds { min: Some(value), max: None }
    }
    pub fn at_most(value: T) -> Self {
        Bounds { min: None, max: Some(value) }
    }
    pub fn is_any(&self) -> bool {
        self.min.is_none() && self.max.is_none()
    }
    pub fn are_swapped(&self) -> bool {
        matches!((self.min, self.max), (Some(a), Some(b)) if a.java_cmp(b) == Ordering::Greater)
    }
    /// `asPoint()`: `min.equals(max) ? min : empty`.
    pub fn as_point(&self) -> Option<T> {
        match (self.min, self.max) {
            (Some(a), Some(b)) if a.java_cmp(b) == Ordering::Equal => Some(a),
            _ => None,
        }
    }
    pub fn map<U: BoundNumber>(&self, f: impl Fn(T) -> U) -> Bounds<U> {
        Bounds { min: self.min.map(&f), max: self.max.map(f) }
    }
    fn validate_swapped(self) -> DataResult<Self> {
        if self.are_swapped() {
            let m = format!("Swapped bounds in range: {} is higher than {}", opt_str(self.min), opt_str(self.max));
            DataResult::error(move || m.clone())
        } else {
            DataResult::success(self)
        }
    }

    /// `Bounds.createCodec(numberCodec)`: `{min?, max?}` or a bare number (exactly that value).
    pub fn create_codec(number: &Codec<T>) -> Codec<Bounds<T>> {
        let range = mc_dfu::record!(Bounds<T> { min: number.optional_field_of("min"), max: number.optional_field_of("max") }).codec();
        range.either(number).xmap(
            |e| match e {
                Either::Left(b) => b,
                Either::Right(v) => Bounds::exactly(v),
            },
            |b: &Bounds<T>| match b.as_point() {
                Some(p) => Either::Right(p),
                None => Either::Left(*b),
            },
        )
    }
}

/// `Bounds.createStreamCodec`: a flag byte (`1` = min, `2` = max) then the present numbers.
fn encode_bounds<T: Copy>(b: &Bounds<T>, out: &mut Vec<u8>, enc: impl Fn(T, &mut Vec<u8>) -> EncodeResult<()>) -> EncodeResult<()> {
    out.push(u8::from(b.min.is_some()) | (u8::from(b.max.is_some()) << 1));
    if let Some(v) = b.min {
        enc(v, out)?;
    }
    if let Some(v) = b.max {
        enc(v, out)?;
    }
    Ok(())
}

fn decode_bounds<T>(input: &mut &[u8], dec: impl Fn(&mut &[u8]) -> DecodeResult<T>) -> DecodeResult<Bounds<T>> {
    let flags = take(input, 1)?[0];
    let min = if flags & 1 != 0 { Some(dec(input)?) } else { None };
    let max = if flags & 2 != 0 { Some(dec(input)?) } else { None };
    Ok(Bounds { min, max })
}

macro_rules! sqr_bounds {
    ($name:ident, $t:ty, $sqr:ty, $num:ident, $sq:expr) => {
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $name {
            pub bounds: Bounds<$t>,
            pub bounds_sqr: Bounds<$sqr>,
        }
        impl $name {
            pub fn new(bounds: Bounds<$t>) -> Self {
                $name { bounds, bounds_sqr: bounds.map($sq) }
            }
            pub fn any() -> Self {
                Self::new(Bounds::any())
            }
            pub fn is_any(&self) -> bool {
                self.bounds.is_any()
            }
            pub fn exactly(v: $t) -> Self {
                Self::new(Bounds::exactly(v))
            }
            pub fn between(min: $t, max: $t) -> Self {
                Self::new(Bounds::between(min, max))
            }
            pub fn at_least(v: $t) -> Self {
                Self::new(Bounds::at_least(v))
            }
            pub fn at_most(v: $t) -> Self {
                Self::new(Bounds::at_most(v))
            }
            pub fn codec() -> Codec<$name> {
                static C: LazyLock<Codec<$name>> = LazyLock::new(|| {
                    Bounds::create_codec(&$num).validate(Bounds::validate_swapped).xmap($name::new, |b: &$name| b.bounds)
                });
                C.clone()
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::any()
            }
        }
    };
}

sqr_bounds!(Ints, i32, i64, INT, |i| i64::from(i) * i64::from(i));
sqr_bounds!(Doubles, f64, f64, DOUBLE, |d| d * d);

impl Ints {
    /// `matches(int)`.
    pub fn matches(&self, value: i32) -> bool {
        !self.bounds.min.is_some_and(|m| m > value) && self.bounds.max.is_none_or(|m| m >= value)
    }
    pub fn matches_sqr(&self, value_sqr: i64) -> bool {
        !self.bounds_sqr.min.is_some_and(|m| m > value_sqr) && self.bounds_sqr.max.is_none_or(|m| m >= value_sqr)
    }
}

impl Doubles {
    pub fn matches(&self, value: f64) -> bool {
        !self.bounds.min.is_some_and(|m| m > value) && self.bounds.max.is_none_or(|m| !(m < value))
    }
    pub fn matches_sqr(&self, value_sqr: f64) -> bool {
        !self.bounds_sqr.min.is_some_and(|m| m > value_sqr) && self.bounds_sqr.max.is_none_or(|m| !(m < value_sqr))
    }
}

/// `MinMaxBounds.FloatDegrees` (no swapped-bounds validation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatDegrees {
    pub bounds: Bounds<f32>,
}

impl FloatDegrees {
    pub fn any() -> Self {
        FloatDegrees { bounds: Bounds::any() }
    }
    pub fn codec() -> Codec<FloatDegrees> {
        static C: LazyLock<Codec<FloatDegrees>> =
            LazyLock::new(|| Bounds::create_codec(&FLOAT).xmap(|bounds| FloatDegrees { bounds }, |b: &FloatDegrees| b.bounds));
        C.clone()
    }
}

impl Encode for Ints {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        encode_bounds(&self.bounds, out, |v, o| v.encode(o))
    }
}
impl Decode for Ints {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        decode_bounds(input, |i| i32::decode(i)).map(Ints::new)
    }
}
impl Encode for Doubles {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        encode_bounds(&self.bounds, out, |v, o| v.encode(o))
    }
}
impl Decode for Doubles {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        decode_bounds(input, |i| f64::decode(i)).map(Doubles::new)
    }
}
impl Encode for FloatDegrees {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        encode_bounds(&self.bounds, out, |v, o| v.encode(o))
    }
}
impl Decode for FloatDegrees {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        decode_bounds(input, |i| f32::decode(i)).map(|bounds| FloatDegrees { bounds })
    }
}
