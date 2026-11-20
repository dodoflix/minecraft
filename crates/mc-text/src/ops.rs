//! The two `DynamicOps` the component codecs run over: `JsonOps` (`mc_core::gson::JsonElement`) and `NbtOps`
//! (`mc_nbt::Tag`), reduced to what `ComponentSerialization` needs. Quirks that matter for
//! behaviour parity (verified against the vanilla oracle):
//!
//! * `JsonOps`: booleans must be real JSON booleans, `null` map values count as absent, only
//!   strings are strings.
//! * `NbtOps`: booleans are any numeric tag (`!= 0.0`), numbers never come from strings, and the
//!   array tags (`[B;]`, `[I;]`, `[L;]`) are also lists.

use crate::component::TranslatableArg;
use crate::value::{Num, big_decimal_low_bits};
use mc_core::gson::{JsonElement, JsonObject};
use mc_core::java_string::JavaString;
use mc_nbt::{CompoundTag, ListTag, Tag};
use serde_json::Number;
use std::borrow::Cow;

/// A map as seen through `DynamicOps.getMap` (keys are always strings here); `owner` is the value it was read from
/// (the `MapLike` that error messages print).
pub struct MapView<'a, O> {
    entries: Vec<(&'a str, &'a O)>,
    owner: &'a O,
}

impl<'a, O> MapView<'a, O> {
    pub fn new(owner: &'a O, entries: Vec<(&'a str, &'a O)>) -> Self {
        MapView { entries, owner }
    }

    pub fn owner(&self) -> &'a O {
        self.owner
    }
}

impl<'a, O> std::ops::Deref for MapView<'a, O> {
    type Target = Vec<(&'a str, &'a O)>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

/// Looks `key` up in a [`MapView`].
pub fn map_get<'a, O>(map: &MapView<'a, O>, key: &str) -> Option<&'a O> {
    map.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// What a `DynamicOps` getter was asked for when it rejected a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    String,
    Number,
    Boolean,
    List,
    /// `getStream` / `getIntStream` (`Codec.INT_STREAM`), which words its error differently from `getList`.
    Stream,
    Map,
}

pub trait Ops: Clone + Sized {
    fn get_string(&self) -> Option<JavaString>;
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
    /// The value as vanilla prints it into `DataResult` messages (`JsonElement.toString` / the SNBT of `Tag.toString`).
    fn render(&self) -> JavaString;
    /// The `DataResult.error` message of the getter for `what` failing on this value.
    fn not_a(&self, what: Prim) -> crate::codec::CodecError;

    fn create_string(s: impl Into<JavaString>) -> Self;
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

/// The number a Gson `LazilyParsedNumber` of this text stands for.
fn json_number(text: &str) -> Option<Number> {
    text.parse::<Number>().ok()
}

fn json_int(text: &str) -> i32 {
    let Some(n) = json_number(text) else {
        return mc_core::gson::lazily_parsed_int_value(text).unwrap_or(0);
    };
    match (n.as_i64(), n.as_u64()) {
        (Some(l), _) => l as i32,
        (None, Some(u)) => u as i32,
        _ => big_decimal_low_bits(n.as_f64().unwrap_or(0.0)) as i32,
    }
}

impl Ops for JsonElement {
    fn normalize_payload(&self, kind: crate::codec::PayloadKind) -> Result<Tag, String> {
        (crate::codec::payload_hooks().from_json)(kind, self)
    }

    fn payload_from_tag(kind: crate::codec::PayloadKind, tag: &Tag) -> crate::codec::R<Self> {
        (crate::codec::payload_hooks().to_json)(kind, tag).map_err(crate::codec::CodecError::new)
    }

    fn render(&self) -> JavaString {
        self.to_java_string()
    }

    fn not_a(&self, what: Prim) -> crate::codec::CodecError {
        let text = match what {
            Prim::String => "Not a string",
            Prim::Number => "Not a number",
            Prim::Boolean => "Not a boolean",
            Prim::List | Prim::Stream => "Not a json array",
            Prim::Map => "Not a JSON object",
        };
        let mut m = JavaString::from(format!("{text}: "));
        m.push_java(&self.to_java_string());
        crate::codec::CodecError(m)
    }

    fn get_string(&self) -> Option<JavaString> {
        match self {
            JsonElement::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn get_bool(&self) -> Option<bool> {
        match self {
            JsonElement::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn get_int(&self) -> Option<i32> {
        match self {
            JsonElement::Number(t) => Some(json_int(t)),
            _ => None,
        }
    }

    fn get_float(&self) -> Option<f32> {
        let JsonElement::Number(t) = self else {
            return None;
        };
        Some(match json_number(t) {
            Some(n) => match (n.as_i64(), n.as_u64()) {
                (Some(l), _) => l as f32,
                (None, Some(u)) => u as f32,
                _ => n.as_f64().unwrap_or(0.0) as f32,
            },
            None => t.parse::<f32>().unwrap_or(0.0),
        })
    }

    fn get_list(&self) -> Option<Vec<Cow<'_, Self>>> {
        match self {
            JsonElement::Array(a) => Some(a.iter().map(Cow::Borrowed).collect()),
            _ => None,
        }
    }

    fn get_map(&self) -> Option<MapView<'_, Self>> {
        // JsonOps' MapLike hides JsonNull values; the field names looked up are ASCII, so the text of a key is enough.
        let JsonElement::Object(o) = self else {
            return None;
        };
        Some(MapView::new(
            self,
            o.iter()
                .filter(|(_, v)| **v != JsonElement::Null)
                .map(|(k, v)| (k.as_lossy_str(), v))
                .collect(),
        ))
    }

    fn get_primitive_arg(&self) -> Option<TranslatableArg> {
        match self {
            JsonElement::String(s) => Some(TranslatableArg::String(s.clone())),
            JsonElement::Bool(b) => Some(TranslatableArg::Bool(*b)),
            JsonElement::Number(t) => Some(TranslatableArg::Number(Num::from_json_text(t))),
            _ => None,
        }
    }

    fn create_string(s: impl Into<JavaString>) -> Self {
        JsonElement::String(s.into())
    }
    fn create_bool(b: bool) -> Self {
        JsonElement::Bool(b)
    }
    fn create_int(v: i32) -> Self {
        JsonElement::Number(v.to_string())
    }
    fn create_int_list(v: &[i32]) -> Self {
        JsonElement::Array(v.iter().map(|i| Self::create_int(*i)).collect())
    }
    fn create_list(items: Vec<Self>) -> Self {
        JsonElement::Array(items)
    }
    fn create_map(entries: Vec<(String, Self)>) -> Self {
        let mut o = JsonObject::new();
        for (k, v) in entries {
            o.add(k, v);
        }
        JsonElement::Object(o)
    }

    fn create_primitive_arg(arg: &TranslatableArg) -> Option<Self> {
        match arg {
            TranslatableArg::String(s) => Some(JsonElement::String(s.clone())),
            TranslatableArg::Bool(b) => Some(JsonElement::Bool(*b)),
            TranslatableArg::Number(n) => n.to_json_text().map(JsonElement::Number),
            TranslatableArg::Component(_) => None,
        }
    }

    fn to_tag(&self) -> Tag {
        match self {
            JsonElement::Null => Tag::End,
            JsonElement::Bool(b) => Tag::Byte(*b as i8),
            JsonElement::Number(t) => num_tag(&Num::from_json_text(t)),
            JsonElement::String(s) => Tag::String(s.clone()),
            JsonElement::Array(a) => Tag::List(ListTag::from(
                a.iter().map(JsonElement::to_tag).collect::<Vec<_>>(),
            )),
            JsonElement::Object(o) => {
                let mut c = CompoundTag::new();
                for (k, v) in o.iter() {
                    c.put(k.clone(), v.to_tag());
                }
                Tag::Compound(c)
            }
        }
    }

    fn from_tag(tag: &Tag) -> Self {
        let num = |n: Num| {
            n.to_json_text()
                .map_or(JsonElement::Null, JsonElement::Number)
        };
        match tag {
            Tag::End => JsonElement::Null,
            Tag::Byte(v) => num(Num::Byte(*v)),
            Tag::Short(v) => num(Num::Short(*v)),
            Tag::Int(v) => num(Num::Int(*v)),
            Tag::Long(v) => num(Num::Long(*v)),
            Tag::Float(v) => num(Num::Float(*v)),
            Tag::Double(v) => num(Num::Double(*v)),
            Tag::ByteArray(a) => JsonElement::Array(a.iter().map(|v| num(Num::Byte(*v))).collect()),
            Tag::IntArray(a) => JsonElement::Array(a.iter().map(|v| num(Num::Int(*v))).collect()),
            Tag::LongArray(a) => JsonElement::Array(a.iter().map(|v| num(Num::Long(*v))).collect()),
            Tag::String(s) => JsonElement::String(s.clone()),
            Tag::List(l) => JsonElement::Array(l.iter().map(JsonElement::from_tag).collect()),
            // CompoundTag is a HashMap in vanilla: JsonOps.convertMap emits keys in Java hash order.
            Tag::Compound(c) => JsonElement::Object(JsonObject::from_distinct_members(
                c.iter_java_order()
                    .into_iter()
                    .map(|(k, v)| (k.clone(), JsonElement::from_tag(v)))
                    .collect(),
            )),
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

    fn payload_from_tag(kind: crate::codec::PayloadKind, tag: &Tag) -> crate::codec::R<Self> {
        if crate::codec::is_context_free() {
            return (crate::codec::payload_hooks().expand)(kind, tag)
                .map_err(crate::codec::CodecError::new);
        }
        Ok(tag.clone())
    }

    fn render(&self) -> JavaString {
        mc_nbt::snbt::print_compact(self)
    }

    fn not_a(&self, what: Prim) -> crate::codec::CodecError {
        let prefix = match what {
            Prim::String => return crate::codec::CodecError::new("Not a string"),
            Prim::Number | Prim::Boolean => return crate::codec::CodecError::new("Not a number"),
            Prim::List => "Not a list: ",
            Prim::Stream => return crate::codec::CodecError::new("Not a list"),
            Prim::Map => "Not a map: ",
        };
        let mut m = JavaString::from(prefix);
        m.push_java(&mc_nbt::snbt::print_compact(self));
        crate::codec::CodecError(m)
    }

    fn get_string(&self) -> Option<JavaString> {
        self.as_java_string().cloned()
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
        self.as_compound()
            .map(|c| MapView::new(self, c.iter().map(|(k, v)| (k.as_lossy_str(), v)).collect()))
    }

    fn get_primitive_arg(&self) -> Option<TranslatableArg> {
        match self {
            Tag::String(s) => Some(TranslatableArg::String(s.clone())),
            other => tag_num(other).map(TranslatableArg::Number),
        }
    }

    fn create_string(s: impl Into<JavaString>) -> Self {
        Tag::String(s.into())
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
