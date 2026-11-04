//! The two `DynamicOps` the component codecs run over: `JsonOps` (`serde_json::Value`) and `NbtOps`
//! (`mc_nbt::Tag`), reduced to what `ComponentSerialization` needs. Quirks that matter for
//! behaviour parity (verified against the vanilla oracle):
//!
//! * `JsonOps`: booleans must be real JSON booleans, `null` map values count as absent, only
//!   strings are strings.
//! * `NbtOps`: booleans are any numeric tag (`!= 0.0`), numbers never come from strings, and the
//!   array tags (`[B;]`, `[I;]`, `[L;]`) are also lists.

use crate::component::TranslatableArg;
use crate::value::{big_decimal_low_bits, Num};
use mc_nbt::{CompoundTag, ListTag, Tag};
use serde_json::{Map, Number, Value};
use std::borrow::Cow;

/// A map as seen through `DynamicOps.getMap` (keys are always strings here).
pub type MapView<'a, O> = Vec<(&'a str, &'a O)>;

/// Looks `key` up in a [`MapView`].
pub fn map_get<'a, O>(map: &MapView<'a, O>, key: &str) -> Option<&'a O> {
    map.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

pub trait Ops: Clone + Sized {
    fn get_string(&self) -> Option<String>;
    fn get_bool(&self) -> Option<bool>;
    /// `Codec.INT` (`getNumberValue().intValue()`).
    fn get_int(&self) -> Option<i32>;
    /// `Codec.FLOAT`.
    fn get_float(&self) -> Option<f32>;
    /// `getList`/`getStream`.
    fn get_list(&self) -> Option<Vec<Cow<'_, Self>>>;
    fn get_map(&self) -> Option<MapView<'_, Self>>;
    /// `ExtraCodecs.JAVA` + `filterAllowedArguments`: a Number/Boolean/String value.
    fn get_primitive_arg(&self) -> Option<TranslatableArg>;

    fn create_string(s: &str) -> Self;
    fn create_bool(b: bool) -> Self;
    fn create_int(v: i32) -> Self;
    /// `createIntList` (`Codec.INT_STREAM`).
    fn create_int_list(v: &[i32]) -> Self;
    fn create_list(items: Vec<Self>) -> Self;
    fn create_map(entries: Vec<(String, Self)>) -> Self;
    /// A primitive `TranslatableArg` converted from `JavaOps` (`None`: not encodable, e.g. NaN in JSON).
    fn create_primitive_arg(arg: &TranslatableArg) -> Option<Self>;

    /// `convertTo(NbtOps.INSTANCE, ..)` (`ExtraCodecs.NBT` decode direction).
    fn to_tag(&self) -> Tag;
    /// `NbtOps.convertTo(<this ops>, tag)` (`ExtraCodecs.NBT` encode direction).
    fn from_tag(tag: &Tag) -> Self;

    /// Typed-codec normalization of a payload of this ops (see [`crate::codec::PayloadHooks`]) -> canonical NBT.
    fn normalize_payload(&self, kind: crate::codec::PayloadKind) -> Result<Tag, String>;
    /// The stored canonical NBT of a payload expressed in this ops.
    fn payload_from_tag(kind: crate::codec::PayloadKind, tag: &Tag) -> crate::codec::R<Self>;
}

// ------------------------------------------------------------------------------------------
// JsonOps
// ------------------------------------------------------------------------------------------

fn json_int(n: &Number) -> i32 {
    match (n.as_i64(), n.as_u64()) {
        (Some(l), _) => l as i32,
        (None, Some(u)) => u as i32,
        _ => big_decimal_low_bits(n.as_f64().unwrap_or(0.0)) as i32,
    }
}

impl Ops for Value {
    fn normalize_payload(&self, kind: crate::codec::PayloadKind) -> Result<Tag, String> {
        (crate::codec::payload_hooks().from_json)(kind, self)
    }

    fn payload_from_tag(kind: crate::codec::PayloadKind, tag: &Tag) -> crate::codec::R<Self> {
        (crate::codec::payload_hooks().to_json)(kind, tag).map_err(crate::codec::CodecError)
    }

    fn get_string(&self) -> Option<String> {
        self.as_str().map(str::to_owned)
    }

    fn get_bool(&self) -> Option<bool> {
        self.as_bool()
    }

    fn get_int(&self) -> Option<i32> {
        self.as_number().map(json_int)
    }

    fn get_float(&self) -> Option<f32> {
        self.as_number().map(|n| match (n.as_i64(), n.as_u64()) {
            (Some(l), _) => l as f32,
            (None, Some(u)) => u as f32,
            _ => n.as_f64().unwrap_or(0.0) as f32,
        })
    }

    fn get_list(&self) -> Option<Vec<Cow<'_, Self>>> {
        self.as_array().map(|a| a.iter().map(Cow::Borrowed).collect())
    }

    fn get_map(&self) -> Option<MapView<'_, Self>> {
        // JsonOps' MapLike hides JsonNull values.
        self.as_object().map(|o| o.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.as_str(), v)).collect())
    }

    fn get_primitive_arg(&self) -> Option<TranslatableArg> {
        match self {
            Value::String(s) => Some(TranslatableArg::String(s.clone())),
            Value::Bool(b) => Some(TranslatableArg::Bool(*b)),
            Value::Number(n) => Some(TranslatableArg::Number(Num::from_json(n))),
            _ => None,
        }
    }

    fn create_string(s: &str) -> Self {
        Value::String(s.to_owned())
    }
    fn create_bool(b: bool) -> Self {
        Value::Bool(b)
    }
    fn create_int(v: i32) -> Self {
        Value::from(v)
    }
    fn create_int_list(v: &[i32]) -> Self {
        Value::Array(v.iter().map(|i| Value::from(*i)).collect())
    }
    fn create_list(items: Vec<Self>) -> Self {
        Value::Array(items)
    }
    fn create_map(entries: Vec<(String, Self)>) -> Self {
        Value::Object(entries.into_iter().collect::<Map<_, _>>())
    }

    fn create_primitive_arg(arg: &TranslatableArg) -> Option<Self> {
        match arg {
            TranslatableArg::String(s) => Some(Value::String(s.clone())),
            TranslatableArg::Bool(b) => Some(Value::Bool(*b)),
            TranslatableArg::Number(n) => n.to_json_number().map(Value::Number),
            TranslatableArg::Component(_) => None,
        }
    }

    fn to_tag(&self) -> Tag {
        match self {
            Value::Null => Tag::End,
            Value::Bool(b) => Tag::Byte(*b as i8),
            Value::Number(n) => match Num::from_json(n) {
                Num::Byte(v) => Tag::Byte(v),
                Num::Short(v) => Tag::Short(v),
                Num::Int(v) => Tag::Int(v),
                Num::Long(v) => Tag::Long(v),
                Num::Float(v) => Tag::Float(v),
                Num::Double(v) => Tag::Double(v),
            },
            Value::String(s) => Tag::String(s.clone()),
            Value::Array(a) => Tag::List(ListTag::from(a.iter().map(Value::to_tag).collect::<Vec<_>>())),
            Value::Object(o) => {
                let mut c = CompoundTag::new();
                for (k, v) in o {
                    c.put(k.clone(), v.to_tag());
                }
                Tag::Compound(c)
            }
        }
    }

    fn from_tag(tag: &Tag) -> Self {
        let num = |n: Num| n.to_json_number().map_or(Value::Null, Value::Number);
        match tag {
            Tag::End => Value::Null,
            Tag::Byte(v) => num(Num::Byte(*v)),
            Tag::Short(v) => num(Num::Short(*v)),
            Tag::Int(v) => num(Num::Int(*v)),
            Tag::Long(v) => num(Num::Long(*v)),
            Tag::Float(v) => num(Num::Float(*v)),
            Tag::Double(v) => num(Num::Double(*v)),
            Tag::ByteArray(a) => Value::Array(a.iter().map(|v| num(Num::Byte(*v))).collect()),
            Tag::IntArray(a) => Value::Array(a.iter().map(|v| num(Num::Int(*v))).collect()),
            Tag::LongArray(a) => Value::Array(a.iter().map(|v| num(Num::Long(*v))).collect()),
            Tag::String(s) => Value::String(s.clone()),
            Tag::List(l) => Value::Array(l.iter().map(Value::from_tag).collect()),
            // CompoundTag is a HashMap in vanilla: JsonOps.convertMap emits keys in Java hash order.
            Tag::Compound(c) => Value::Object(c.iter_java_order().into_iter().map(|(k, v)| (k.clone(), Value::from_tag(v))).collect()),
        }
    }
}

// ------------------------------------------------------------------------------------------
// NbtOps
// ------------------------------------------------------------------------------------------

fn tag_num(tag: &Tag) -> Option<Num> {
    Some(match tag {
        Tag::Byte(v) => Num::Byte(*v),
        Tag::Short(v) => Num::Short(*v),
        Tag::Int(v) => Num::Int(*v),
        Tag::Long(v) => Num::Long(*v),
        Tag::Float(v) => Num::Float(*v),
        Tag::Double(v) => Num::Double(*v),
        _ => return None,
    })
}

fn num_tag(n: &Num) -> Tag {
    match n {
        Num::Byte(v) => Tag::Byte(*v),
        Num::Short(v) => Tag::Short(*v),
        Num::Int(v) => Tag::Int(*v),
        Num::Long(v) => Tag::Long(*v),
        Num::Float(v) => Tag::Float(*v),
        Num::Double(v) => Tag::Double(*v),
    }
}

impl Ops for Tag {
    fn normalize_payload(&self, kind: crate::codec::PayloadKind) -> Result<Tag, String> {
        (crate::codec::payload_hooks().from_nbt)(kind, self)
    }

    fn payload_from_tag(_kind: crate::codec::PayloadKind, tag: &Tag) -> crate::codec::R<Self> {
        Ok(tag.clone())
    }

    fn get_string(&self) -> Option<String> {
        self.as_string().map(str::to_owned)
    }

    fn get_bool(&self) -> Option<bool> {
        // NbtOps.getBooleanValue: numeric tag with doubleValue() != 0.0
        tag_num(self).map(|n| n.double_value() != 0.0)
    }

    fn get_int(&self) -> Option<i32> {
        tag_num(self).map(|n| n.int_value())
    }

    fn get_float(&self) -> Option<f32> {
        tag_num(self).map(|n| match n {
            Num::Byte(v) => v as f32,
            Num::Short(v) => v as f32,
            Num::Int(v) => v as f32,
            Num::Long(v) => v as f32,
            Num::Float(v) => v,
            Num::Double(v) => v as f32,
        })
    }

    fn get_list(&self) -> Option<Vec<Cow<'_, Self>>> {
        match self {
            Tag::List(l) => Some(l.iter().map(Cow::Borrowed).collect()),
            Tag::ByteArray(a) => Some(a.iter().map(|v| Cow::Owned(Tag::Byte(*v))).collect()),
            Tag::IntArray(a) => Some(a.iter().map(|v| Cow::Owned(Tag::Int(*v))).collect()),
            Tag::LongArray(a) => Some(a.iter().map(|v| Cow::Owned(Tag::Long(*v))).collect()),
            _ => None,
        }
    }

    fn get_map(&self) -> Option<MapView<'_, Self>> {
        self.as_compound().map(|c| c.iter().map(|(k, v)| (k.as_str(), v)).collect())
    }

    fn get_primitive_arg(&self) -> Option<TranslatableArg> {
        match self {
            Tag::String(s) => Some(TranslatableArg::String(s.clone())),
            other => tag_num(other).map(TranslatableArg::Number),
        }
    }

    fn create_string(s: &str) -> Self {
        Tag::String(s.to_owned())
    }
    fn create_bool(b: bool) -> Self {
        Tag::Byte(b as i8)
    }
    fn create_int(v: i32) -> Self {
        Tag::Int(v)
    }
    fn create_int_list(v: &[i32]) -> Self {
        Tag::IntArray(v.to_vec())
    }
    fn create_list(items: Vec<Self>) -> Self {
        Tag::List(ListTag::from(items))
    }
    fn create_map(entries: Vec<(String, Self)>) -> Self {
        let mut c = CompoundTag::new();
        for (k, v) in entries {
            c.put(k, v);
        }
        Tag::Compound(c)
    }

    fn create_primitive_arg(arg: &TranslatableArg) -> Option<Self> {
        match arg {
            TranslatableArg::String(s) => Some(Tag::String(s.clone())),
            TranslatableArg::Bool(b) => Some(Tag::Byte(*b as i8)),
            TranslatableArg::Number(n) => Some(num_tag(n)),
            TranslatableArg::Component(_) => None,
        }
    }

    fn to_tag(&self) -> Tag {
        self.clone()
    }

    fn from_tag(tag: &Tag) -> Self {
        tag.clone()
    }
}
