//! Shared building blocks of component values: enums, text components, sound events, registry sets.

use crate::codec::{read_varint, write_varint, Decode, DecodeError, DecodeResult, Encode};
use crate::components::holder::{self, Holder};
use mc_core::ident::Identifier;
use mc_data::registry::RegistryId;
use mc_dfu::{Codec, DataResult, Dyn, Either, OpsKind};
use mc_nbt::Tag;
use mc_text::Component;

/// Declares a `StringRepresentable` enum: variants with their serialized names, `ALL`, `id()` (ordinal), `name()`,
/// `from_id`, `from_name`, `codec()` (string form). The stream form is written per type (out-of-bounds strategies differ).
#[macro_export]
macro_rules! string_enum {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($variant:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        $vis enum $name { $($variant),+ }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            const NAMES: &'static [($name, &'static str)] = &[$(($name::$variant, $s)),+];
            /// Ordinal.
            pub fn id(self) -> i32 { self as i32 }
            pub fn from_id(id: i32) -> Option<$name> { usize::try_from(id).ok().and_then(|i| Self::ALL.get(i).copied()) }
            /// Serialized name.
            pub fn name(self) -> &'static str { Self::NAMES[self as usize].1 }
            pub fn from_name(name: &str) -> Option<$name> { Self::NAMES.iter().find(|(_, n)| *n == name).map(|(v, _)| *v) }
            /// `StringRepresentable.fromEnum(values)` codec.
            pub fn codec() -> mc_dfu::Codec<$name> { mc_dfu::string_enum_codec(Self::NAMES) }
        }
    };
}

/// `ByIdMap.OutOfBoundsStrategy.ZERO` read of an ordinal: `0` when out of range.
pub fn read_enum_zero<T: Copy>(input: &mut &[u8], all: &[T]) -> DecodeResult<T> {
    let id = read_varint(input)?;
    Ok(usize::try_from(id).ok().and_then(|i| all.get(i).copied()).unwrap_or(all[0]))
}

string_enum! {
    /// `net.minecraft.world.item.DyeColor`.
    pub enum DyeColor {
        White = "white", Orange = "orange", Magenta = "magenta", LightBlue = "light_blue", Yellow = "yellow",
        Lime = "lime", Pink = "pink", Gray = "gray", LightGray = "light_gray", Cyan = "cyan", Purple = "purple",
        Blue = "blue", Brown = "brown", Green = "green", Red = "red", Black = "black",
    }
}

impl Encode for DyeColor {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.id())
    }
}
impl Decode for DyeColor {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_enum_zero(input, DyeColor::ALL)
    }
}

string_enum! {
    /// `net.minecraft.world.item.Rarity`.
    pub enum Rarity { Common = "common", Uncommon = "uncommon", Rare = "rare", Epic = "epic" }
}

impl Encode for Rarity {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.id())
    }
}
impl Decode for Rarity {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        read_enum_zero(input, Rarity::ALL)
    }
}

// ---- Dyn <-> serde_json (the text component codecs speak serde_json) -------------------------------------------

pub(crate) fn dyn_to_serde(d: &Dyn) -> serde_json::Value {
    use serde_json::{Number, Value};
    match d {
        Dyn::Empty => Value::Null,
        Dyn::Bool(b) => Value::Bool(*b),
        Dyn::Byte(v) => Value::from(*v as i64),
        Dyn::Short(v) => Value::from(*v as i64),
        Dyn::Int(v) => Value::from(*v as i64),
        Dyn::Long(v) => Value::from(*v),
        Dyn::Float(v) => Number::from_f64(*v as f64).map(Value::Number).unwrap_or(Value::Null),
        Dyn::Double(v) => Number::from_f64(*v).map(Value::Number).unwrap_or(Value::Null),
        Dyn::JsonNum(t) => t
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| t.parse::<u64>().map(Value::from))
            .or_else(|_| t.parse::<f64>().map(|f| Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)))
            .unwrap_or(Value::Null),
        Dyn::Str(s) => Value::String(s.clone()),
        Dyn::List(l) => Value::Array(l.iter().map(dyn_to_serde).collect()),
        Dyn::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.as_str().unwrap_or("").to_string(), dyn_to_serde(v))).collect()),
        Dyn::Bytes(v) => Value::Array(v.iter().map(|x| Value::from(*x as i64)).collect()),
        Dyn::Ints(v) => Value::Array(v.iter().map(|x| Value::from(*x as i64)).collect()),
        Dyn::Longs(v) => Value::Array(v.iter().map(|x| Value::from(*x)).collect()),
    }
}

pub(crate) fn serde_to_dyn(v: &serde_json::Value) -> Dyn {
    use serde_json::Value;
    match v {
        Value::Null => Dyn::Empty,
        Value::Bool(b) => Dyn::Bool(*b),
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => Dyn::Long(i),
            (None, Some(f)) => Dyn::Double(f),
            _ => Dyn::JsonNum(n.to_string()),
        },
        Value::String(s) => Dyn::Str(s.clone()),
        Value::Array(a) => Dyn::List(a.iter().map(serde_to_dyn).collect()),
        Value::Object(o) => Dyn::Map(o.iter().map(|(k, v)| (Dyn::Str(k.clone()), serde_to_dyn(v))).collect()),
    }
}

/// NBT encodes booleans as bytes but the hash ops hash real booleans: walk the NBT tree together with the JSON
/// encoding of the same component (where booleans are real) and turn the bytes that are JSON booleans back.
fn mark_bools(nbt: Dyn, json: &Dyn) -> Dyn {
    match (nbt, json) {
        (Dyn::Byte(b), Dyn::Bool(_)) => Dyn::Bool(b != 0),
        (Dyn::Map(m), Dyn::Map(jm)) => Dyn::Map(
            m.into_iter()
                .map(|(k, v)| {
                    let j = jm.iter().find(|(jk, _)| *jk == k).map(|(_, jv)| jv);
                    let v = match j {
                        Some(j) => mark_bools(v, j),
                        None => v,
                    };
                    (k, v)
                })
                .collect(),
        ),
        (Dyn::List(l), Dyn::List(jl)) if l.len() == jl.len() => {
            Dyn::List(l.into_iter().zip(jl.iter()).map(|(v, j)| mark_bools(v, j)).collect())
        }
        (other, _) => other,
    }
}

/// `ComponentSerialization.CODEC` over all ops (see `mc_text`).
pub fn component_codec() -> Codec<Component> {
    Codec::from_fns(
        |ops, input: &Component, prefix| {
            let encoded: Result<Dyn, String> = match ops.kind() {
                OpsKind::Json => input.to_json().map(|v| serde_to_dyn(&v)).map_err(|e| e.to_string()),
                OpsKind::Nbt => input.to_nbt().map(|t| Dyn::from(&t)).map_err(|e| e.to_string()),
                OpsKind::Hash => input.to_nbt().map_err(|e| e.to_string()).map(|t| {
                    let nbt = Dyn::from(&t);
                    match input.to_json() {
                        Ok(j) => mark_bools(nbt, &serde_to_dyn(&j)),
                        Err(_) => nbt,
                    }
                }),
            };
            match encoded {
                Ok(d) => ops.merge_to_primitive(&prefix, d),
                Err(e) => DataResult::error(move || e.clone()),
            }
        },
        |ops, input| {
            let r: Result<Component, String> = match ops.kind() {
                OpsKind::Json => Component::from_json(&dyn_to_serde(input)).map_err(|e| e.to_string()),
                OpsKind::Nbt => match input.to_tag() {
                    Ok(t) => Component::from_nbt(&t).map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                },
                OpsKind::Hash => Err("Unsupported operation".into()),
            };
            match r {
                Ok(c) => DataResult::success((c, ops.empty())),
                Err(e) => DataResult::error(move || e.clone()),
            }
        },
    )
}

/// `Tag` value of a text component for stream use is `Component: Encode + Decode` (see `codec::text`).
#[allow(dead_code)]
fn _tag_marker(_: Tag) {}

// ---- sound events ------------------------------------------------------------------------------------------------

/// `SoundEvent` (a registered or inline sound): `location` + optional fixed range.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundEvent {
    pub location: Identifier,
    pub fixed_range: Option<f32>,
}

impl SoundEvent {
    /// `SoundEvent.DIRECT_CODEC`.
    pub fn direct_codec() -> Codec<SoundEvent> {
        mc_dfu::record!(SoundEvent {
            location: mc_dfu::extra::IDENTIFIER.field_of("sound_id"),
            fixed_range: mc_dfu::FLOAT.lenient_optional_field_of("range"),
        })
        .codec()
    }
    /// `SoundEvent.CODEC`: a registered sound id or an inline definition.
    pub fn holder_codec() -> Codec<Holder<SoundEvent>> {
        holder::holder_file_codec("minecraft:sound_event", &Self::direct_codec(), true)
    }
    pub fn encode_direct(&self, out: &mut Vec<u8>) {
        self.location.encode(out);
        match self.fixed_range {
            Some(r) => {
                out.push(1);
                r.encode(out);
            }
            None => out.push(0),
        }
    }
    pub fn decode_direct(input: &mut &[u8]) -> DecodeResult<SoundEvent> {
        let location = Identifier::decode(input)?;
        let fixed_range = if bool::decode(input)? { Some(f32::decode(input)?) } else { None };
        Ok(SoundEvent { location, fixed_range })
    }
    /// `SoundEvent.STREAM_CODEC`.
    pub fn encode_holder(h: &Holder<SoundEvent>, out: &mut Vec<u8>) {
        holder::encode_holder("minecraft:sound_event", h, out, |v, o| v.encode_direct(o))
    }
    pub fn decode_holder(input: &mut &[u8]) -> DecodeResult<Holder<SoundEvent>> {
        holder::decode_holder("minecraft:sound_event", input, SoundEvent::decode_direct)
    }
}

// ---- registry sets of built-in registries --------------------------------------------------------------------------

/// `HolderSet<T>` of a *built-in* registry with typed entries (blocks, items, entity types, ...): a tag or a list.
#[derive(Clone, Debug, PartialEq)]
pub enum RegistrySet<T> {
    Tag(Identifier),
    Entries(Vec<T>),
}

impl<T: RegistryId> RegistrySet<T> {
    /// `RegistryCodecs.homogeneousList(key)`: tag (`#id`), list, or a single id.
    pub fn codec() -> Codec<RegistrySet<T>> {
        Self::codec_with(false)
    }
    /// `HolderSetCodec.create(key, byNameCodec, alwaysUseList)`.
    pub fn codec_with(always_use_list: bool) -> Codec<RegistrySet<T>> {
        let registry = T::registry().name();
        let element = holder::registry_by_name_codec::<T>();
        let list = element.list_of();
        let direct = if always_use_list { list } else { mc_dfu::extra::compact_list_codec_with(&element, &list) };
        let tag = hashed_tag_codec();
        let either = tag.either(&direct);
        let (e1, e2) = (either.clone(), either);
        Codec::from_fns(
            move |ops, input: &RegistrySet<T>, prefix| match input {
                RegistrySet::Tag(t) => e1.encode(ops, &Either::Left(t.clone()), prefix),
                RegistrySet::Entries(v) => e1.encode(ops, &Either::Right(v.clone()), prefix),
            },
            move |ops, input| {
                e2.decode(ops, input).flat_map(|(either, rest)| match either {
                    Either::Left(tag) => {
                        if crate::registry_context::get().tag_exists(registry, &tag) {
                            DataResult::success((RegistrySet::Tag(tag), rest))
                        } else {
                            DataResult::error(move || format!("Missing tag: '{tag}' in '{registry}'"))
                        }
                    }
                    Either::Right(v) => DataResult::success((RegistrySet::Entries(v), rest)),
                })
            },
        )
    }

    /// Whether `value` is in the set (tags resolved against the embedded vanilla data pack).
    pub fn contains(&self, value: T) -> bool {
        match self {
            RegistrySet::Entries(v) => v.contains(&value),
            RegistrySet::Tag(t) => mc_data::tags::get::<T>(&t.to_string()).is_some_and(|s| s.contains(value)),
        }
    }

    /// `ByteBufCodecs.holderSet(key)`.
    pub fn encode_stream(&self, out: &mut Vec<u8>) {
        match self {
            RegistrySet::Tag(t) => {
                write_varint(out, 0);
                t.encode(out);
            }
            RegistrySet::Entries(v) => {
                write_varint(out, v.len() as i32 + 1);
                for e in v {
                    write_varint(out, e.id() as i32);
                }
            }
        }
    }
    pub fn decode_stream(input: &mut &[u8]) -> DecodeResult<RegistrySet<T>> {
        let count = i64::from(read_varint(input)?) - 1;
        if count == -1 {
            // `registry.get(TagKey).orElseThrow()`
            let tag = Identifier::decode(input)?;
            if !crate::registry_context::get().tag_exists(T::registry().name(), &tag) {
                return Err(DecodeError::msg("No value present"));
            }
            return Ok(RegistrySet::Tag(tag));
        }
        if count < -1 {
            return Err(DecodeError::Message(format!("Illegal Capacity: {count}")));
        }
        let mut v = Vec::with_capacity((count as usize).min(65536));
        for _ in 0..count {
            v.push(crate::codec::read_registry_id::<T>(input)?);
        }
        Ok(RegistrySet::Entries(v))
    }
}

pub(crate) fn hashed_tag_codec() -> Codec<Identifier> {
    mc_dfu::STRING.comap_flat_map(
        |s| match s.strip_prefix('#') {
            None => DataResult::error(move || format!("Not a tag id: {s}")),
            Some(rest) => match Identifier::read(rest) {
                Ok(id) => DataResult::success(id),
                Err(e) => DataResult::error(move || e.clone()),
            },
        },
        |id: &Identifier| format!("#{id}"),
    )
}

// ---- compound tag codecs ----------------------------------------------------------------------------------------

fn json_null_map_value(d: &Dyn) -> bool {
    match d {
        Dyn::Map(m) => m.iter().any(|(_, v)| matches!(v, Dyn::Empty) || json_null_map_value(v)),
        Dyn::List(l) => l.iter().any(json_null_map_value),
        _ => false,
    }
}

/// `CompoundTag.CODEC`: any value that converts to an NBT compound.
pub fn compound_tag_codec() -> Codec<mc_nbt::CompoundTag> {
    mc_dfu::PASSTHROUGH.flat_xmap(
        |d: mc_dfu::Dynamic| {
            // JsonOps.getMapValues turns a JSON null value into Java null, which `convertTo` then dereferences (NPE)
            if d.ops.kind() == OpsKind::Json && json_null_map_value(&d.value) {
                return DataResult::error_str("Cannot convert a JSON null map value");
            }
            let converted = d.ops.convert_to(&mc_dfu::NBT, &d.value);
            match converted.to_tag() {
                Ok(Tag::Compound(c)) => DataResult::success(c),
                Ok(other) => DataResult::error(move || format!("Not a compound tag: {other}")),
                Err(e) => DataResult::error(move || e.clone()),
            }
        },
        |c: &mc_nbt::CompoundTag| DataResult::success(mc_dfu::Dynamic::new(mc_dfu::NBT, Dyn::from(&Tag::Compound(c.clone())))),
    )
}

/// `TagParser.FLATTENED_CODEC`: a compound as an SNBT string.
pub fn flattened_compound_codec() -> Codec<mc_nbt::CompoundTag> {
    mc_dfu::STRING.comap_flat_map(
        |s| match mc_nbt::snbt::parse(&s) {
            Ok(Tag::Compound(c)) => DataResult::success(c),
            Ok(other) => DataResult::error(move || format!("Expected compound tag, got {other}")),
            Err(e) => DataResult::error(move || e.to_string()),
        },
        |c: &mc_nbt::CompoundTag| Tag::Compound(c.clone()).to_string(),
    )
}

/// `TagParser.LENIENT_CODEC`: `Codec.withAlternative(FLATTENED_CODEC, CompoundTag.CODEC)`.
pub fn lenient_compound_codec() -> Codec<mc_nbt::CompoundTag> {
    flattened_compound_codec().with_alternative(&compound_tag_codec())
}

/// `CustomData.COMPOUND_TAG_CODEC`: `Codec.withAlternative(CompoundTag.CODEC, TagParser.FLATTENED_CODEC)`.
pub fn custom_data_compound_tag_codec() -> Codec<mc_nbt::CompoundTag> {
    compound_tag_codec().with_alternative(&flattened_compound_codec())
}

/// `UUIDUtil.CODEC`: an int array of exactly four ints.
pub fn uuid_codec() -> Codec<uuid::Uuid> {
    mc_dfu::INT_STREAM.comap_flat_map(
        |ints: Vec<i32>| match <[i32; 4]>::try_from(&ints[..ints.len().min(4)]) {
            Ok([a, b, c, d]) => {
                let uuid = uuid::Uuid::from_u64_pair(
                    ((a as u64) << 32) | (b as u32 as u64),
                    ((c as u64) << 32) | (d as u32 as u64),
                );
                if ints.len() == 4 {
                    DataResult::success(uuid)
                } else {
                    DataResult::error_partial(|| "Input is not a list of 4 ints".to_string(), uuid)
                }
            }
            Err(_) => DataResult::error_str("Input is not a list of 4 ints"),
        },
        |u: &uuid::Uuid| {
            let (most, least) = u.as_u64_pair();
            vec![(most >> 32) as i32, most as i32, (least >> 32) as i32, least as i32]
        },
    )
}
