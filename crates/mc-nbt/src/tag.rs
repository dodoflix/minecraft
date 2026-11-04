//! `net.minecraft.nbt.Tag` and the numeric-tag value semantics.

use crate::{CompoundTag, ListTag};
use std::fmt;

pub const TAG_END: u8 = 0;
pub const TAG_BYTE: u8 = 1;
pub const TAG_SHORT: u8 = 2;
pub const TAG_INT: u8 = 3;
pub const TAG_LONG: u8 = 4;
pub const TAG_FLOAT: u8 = 5;
pub const TAG_DOUBLE: u8 = 6;
pub const TAG_BYTE_ARRAY: u8 = 7;
pub const TAG_STRING: u8 = 8;
pub const TAG_LIST: u8 = 9;
pub const TAG_COMPOUND: u8 = 10;
pub const TAG_INT_ARRAY: u8 = 11;
pub const TAG_LONG_ARRAY: u8 = 12;
/// `Tag.MAX_DEPTH`.
pub const MAX_DEPTH: i32 = 512;

/// An NBT tag. Equality follows Java `equals` of the vanilla records (floats compare with
/// `Float.compare` semantics: `NaN == NaN`, `0.0 != -0.0`); compounds compare order-independently.
#[derive(Debug, Clone)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(ListTag),
    Compound(CompoundTag),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

/// The value of a `NumericTag` with vanilla's narrowing rules. `*_value` are the tag methods
/// (`NumericTag.intValue()`: floats are floored); `boxed_*_value` are `java.lang.Number`'s
/// (`NumericTag.box()`: floats are truncated).
#[derive(Debug, Clone, Copy)]
pub enum Number {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
}

/// `Mth.floor(float)` = `(int) Math.floor(v)` (saturating).
fn floor_f32(v: f32) -> i32 {
    (v as f64).floor() as i32
}

/// `Mth.floor(double)`.
fn floor_f64(v: f64) -> i32 {
    v.floor() as i32
}

impl Number {
    pub fn byte_value(self) -> i8 {
        match self {
            Number::Byte(v) => v,
            Number::Short(v) => v as i8,
            Number::Int(v) => v as i8,
            Number::Long(v) => v as i8,
            Number::Float(v) => floor_f32(v) as i8,
            Number::Double(v) => floor_f64(v) as i8,
        }
    }

    pub fn short_value(self) -> i16 {
        match self {
            Number::Byte(v) => v as i16,
            Number::Short(v) => v,
            Number::Int(v) => v as i16,
            Number::Long(v) => v as i16,
            Number::Float(v) => floor_f32(v) as i16,
            Number::Double(v) => floor_f64(v) as i16,
        }
    }

    pub fn int_value(self) -> i32 {
        match self {
            Number::Byte(v) => v as i32,
            Number::Short(v) => v as i32,
            Number::Int(v) => v,
            Number::Long(v) => v as i32,
            Number::Float(v) => floor_f32(v),
            Number::Double(v) => floor_f64(v),
        }
    }

    pub fn long_value(self) -> i64 {
        match self {
            Number::Byte(v) => v as i64,
            Number::Short(v) => v as i64,
            Number::Int(v) => v as i64,
            Number::Long(v) => v,
            Number::Float(v) => v as i64,
            Number::Double(v) => v.floor() as i64,
        }
    }

    pub fn float_value(self) -> f32 {
        match self {
            Number::Byte(v) => v as f32,
            Number::Short(v) => v as f32,
            Number::Int(v) => v as f32,
            Number::Long(v) => v as f32,
            Number::Float(v) => v,
            Number::Double(v) => v as f32,
        }
    }

    pub fn double_value(self) -> f64 {
        match self {
            Number::Byte(v) => v as f64,
            Number::Short(v) => v as f64,
            Number::Int(v) => v as f64,
            Number::Long(v) => v as f64,
            Number::Float(v) => v as f64,
            Number::Double(v) => v,
        }
    }

    /// `java.lang.Number.byteValue()` on the boxed value (`NumericTag.box()`): floats/doubles
    /// *truncate* (`(byte)(int) d`), unlike the tag's own `byteValue()` ([`Number::byte_value`]) which floors.
    /// Codec paths (`NbtOps.getNumberValue`) see the boxed form.
    pub fn boxed_byte_value(self) -> i8 {
        self.boxed_int_value() as i8
    }

    /// Boxed `Number.shortValue()` (truncating for floats).
    pub fn boxed_short_value(self) -> i16 {
        self.boxed_int_value() as i16
    }

    /// Boxed `Number.intValue()` (truncating for floats).
    pub fn boxed_int_value(self) -> i32 {
        match self {
            Number::Float(v) => v as i32,
            Number::Double(v) => v as i32,
            other => other.int_value(),
        }
    }

    /// Boxed `Number.longValue()` (truncating for floats).
    pub fn boxed_long_value(self) -> i64 {
        match self {
            Number::Float(v) => v as i64,
            Number::Double(v) => v as i64,
            other => other.long_value(),
        }
    }

    /// `NumericTag.asBoolean()`: `byteValue() != 0`.
    pub fn boolean_value(self) -> bool {
        self.byte_value() != 0
    }
}

pub(crate) fn f32_eq(a: f32, b: f32) -> bool {
    // Float.compare(a, b) == 0 via floatToIntBits (canonical NaN).
    let canon = |v: f32| if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() };
    canon(a) == canon(b)
}

pub(crate) fn f64_eq(a: f64, b: f64) -> bool {
    let canon = |v: f64| if v.is_nan() { 0x7ff8_0000_0000_0000 } else { v.to_bits() };
    canon(a) == canon(b)
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Tag::End, Tag::End) => true,
            (Tag::Byte(a), Tag::Byte(b)) => a == b,
            (Tag::Short(a), Tag::Short(b)) => a == b,
            (Tag::Int(a), Tag::Int(b)) => a == b,
            (Tag::Long(a), Tag::Long(b)) => a == b,
            (Tag::Float(a), Tag::Float(b)) => f32_eq(*a, *b),
            (Tag::Double(a), Tag::Double(b)) => f64_eq(*a, *b),
            (Tag::ByteArray(a), Tag::ByteArray(b)) => a == b,
            (Tag::String(a), Tag::String(b)) => a == b,
            (Tag::List(a), Tag::List(b)) => a == b,
            (Tag::Compound(a), Tag::Compound(b)) => a == b,
            (Tag::IntArray(a), Tag::IntArray(b)) => a == b,
            (Tag::LongArray(a), Tag::LongArray(b)) => a == b,
            _ => false,
        }
    }
}

impl Tag {
    /// `Tag.getId()`.
    pub fn id(&self) -> u8 {
        match self {
            Tag::End => TAG_END,
            Tag::Byte(_) => TAG_BYTE,
            Tag::Short(_) => TAG_SHORT,
            Tag::Int(_) => TAG_INT,
            Tag::Long(_) => TAG_LONG,
            Tag::Float(_) => TAG_FLOAT,
            Tag::Double(_) => TAG_DOUBLE,
            Tag::ByteArray(_) => TAG_BYTE_ARRAY,
            Tag::String(_) => TAG_STRING,
            Tag::List(_) => TAG_LIST,
            Tag::Compound(_) => TAG_COMPOUND,
            Tag::IntArray(_) => TAG_INT_ARRAY,
            Tag::LongArray(_) => TAG_LONG_ARRAY,
        }
    }

    /// `TagType.getName()` for a type id (`INVALID[id]` when unknown).
    pub fn type_name(id: u8) -> String {
        match id {
            0 => "END".into(),
            1 => "BYTE".into(),
            2 => "SHORT".into(),
            3 => "INT".into(),
            4 => "LONG".into(),
            5 => "FLOAT".into(),
            6 => "DOUBLE".into(),
            7 => "BYTE[]".into(),
            8 => "STRING".into(),
            9 => "LIST".into(),
            10 => "COMPOUND".into(),
            11 => "INT[]".into(),
            12 => "LONG[]".into(),
            _ => format!("INVALID[{id}]"),
        }
    }

    /// `TagType.getPrettyName()` for a type id (`UNKNOWN_id` when unknown).
    pub fn pretty_type_name(id: u8) -> String {
        match id {
            0 => "TAG_End".into(),
            1 => "TAG_Byte".into(),
            2 => "TAG_Short".into(),
            3 => "TAG_Int".into(),
            4 => "TAG_Long".into(),
            5 => "TAG_Float".into(),
            6 => "TAG_Double".into(),
            7 => "TAG_Byte_Array".into(),
            8 => "TAG_String".into(),
            9 => "TAG_List".into(),
            10 => "TAG_Compound".into(),
            11 => "TAG_Int_Array".into(),
            12 => "TAG_Long_Array".into(),
            _ => format!("UNKNOWN_{id}"),
        }
    }

    /// `Tag.sizeInBytes()`: vanilla's heap-size estimate used by the accounter.
    pub fn size_in_bytes(&self) -> i32 {
        match self {
            Tag::End => 8,
            Tag::Byte(_) => 9,
            Tag::Short(_) => 10,
            Tag::Int(_) | Tag::Float(_) => 12,
            Tag::Long(_) | Tag::Double(_) => 16,
            Tag::ByteArray(v) => 24 + v.len() as i32,
            Tag::String(s) => 36 + 2 * s.encode_utf16().count() as i32,
            Tag::List(l) => l.size_in_bytes(),
            Tag::Compound(c) => c.size_in_bytes(),
            Tag::IntArray(v) => 24 + 4 * v.len() as i32,
            Tag::LongArray(v) => 24 + 8 * v.len() as i32,
        }
    }

    /// `Tag.copy()`.
    pub fn copy(&self) -> Tag {
        self.clone()
    }

    pub fn is_numeric(&self) -> bool {
        self.as_number().is_some()
    }

    /// `Tag.asNumber()` (numeric tags only).
    pub fn as_number(&self) -> Option<Number> {
        match *self {
            Tag::Byte(v) => Some(Number::Byte(v)),
            Tag::Short(v) => Some(Number::Short(v)),
            Tag::Int(v) => Some(Number::Int(v)),
            Tag::Long(v) => Some(Number::Long(v)),
            Tag::Float(v) => Some(Number::Float(v)),
            Tag::Double(v) => Some(Number::Double(v)),
            _ => None,
        }
    }

    pub fn as_byte(&self) -> Option<i8> {
        self.as_number().map(Number::byte_value)
    }
    pub fn as_short(&self) -> Option<i16> {
        self.as_number().map(Number::short_value)
    }
    pub fn as_int(&self) -> Option<i32> {
        self.as_number().map(Number::int_value)
    }
    pub fn as_long(&self) -> Option<i64> {
        self.as_number().map(Number::long_value)
    }
    pub fn as_float(&self) -> Option<f32> {
        self.as_number().map(Number::float_value)
    }
    pub fn as_double(&self) -> Option<f64> {
        self.as_number().map(Number::double_value)
    }
    pub fn as_boolean(&self) -> Option<bool> {
        self.as_number().map(Number::boolean_value)
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_byte_array(&self) -> Option<&[i8]> {
        match self {
            Tag::ByteArray(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_int_array(&self) -> Option<&[i32]> {
        match self {
            Tag::IntArray(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_long_array(&self) -> Option<&[i64]> {
        match self {
            Tag::LongArray(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_compound(&self) -> Option<&CompoundTag> {
        match self {
            Tag::Compound(c) => Some(c),
            _ => None,
        }
    }
    pub fn as_compound_mut(&mut self) -> Option<&mut CompoundTag> {
        match self {
            Tag::Compound(c) => Some(c),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&ListTag> {
        match self {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }
    pub fn as_list_mut(&mut self) -> Option<&mut ListTag> {
        match self {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }

    /// `FloatTag.valueOf`: negative zero collapses to `0.0` (vanilla's shared `ZERO` instance).
    pub fn float(v: f32) -> Tag {
        Tag::Float(if v == 0.0 { 0.0 } else { v })
    }

    /// `DoubleTag.valueOf`: negative zero collapses to `0.0`.
    pub fn double(v: f64) -> Tag {
        Tag::Double(if v == 0.0 { 0.0 } else { v })
    }

    /// `ByteTag.valueOf(boolean)`.
    pub fn from_bool(v: bool) -> Tag {
        Tag::Byte(v as i8)
    }
}

/// Compact SNBT (`Tag.toString()` / `StringTagVisitor`).
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&crate::snbt::print_compact(self))
    }
}

macro_rules! tag_from {
    ($($t:ty => $v:ident),* $(,)?) => {$(
        impl From<$t> for Tag {
            fn from(v: $t) -> Tag { Tag::$v(v.into()) }
        }
    )*};
}
tag_from!(i8 => Byte, i16 => Short, i32 => Int, i64 => Long, String => String, &str => String,
    ListTag => List, CompoundTag => Compound);

impl From<f32> for Tag {
    fn from(v: f32) -> Tag {
        Tag::float(v)
    }
}
impl From<f64> for Tag {
    fn from(v: f64) -> Tag {
        Tag::double(v)
    }
}
impl From<bool> for Tag {
    fn from(v: bool) -> Tag {
        Tag::from_bool(v)
    }
}
impl From<Vec<i8>> for Tag {
    fn from(v: Vec<i8>) -> Tag {
        Tag::ByteArray(v)
    }
}
impl From<Vec<i32>> for Tag {
    fn from(v: Vec<i32>) -> Tag {
        Tag::IntArray(v)
    }
}
impl From<Vec<i64>> for Tag {
    fn from(v: Vec<i64>) -> Tag {
        Tag::LongArray(v)
    }
}
