//! `Holder` / `HolderSet` as component values, with the codecs vanilla builds for them (`RegistryFixedCodec`,
//! `RegistryFileCodec`, `HolderSetCodec`, `ByteBufCodecs.holder*`).
//!
//! Registry entries are referenced by *name*; the protocol ids come from [`crate::registry_context`] (data-driven
//! registries) or `mc_data` (built-in ones). Tags are never resolved here: a named set is just the tag id.

use crate::codec::EncodeResult;
use crate::codec::{Decode, DecodeError, DecodeResult, Encode, read_varint, write_varint};
use crate::registry_context as ctx;
use mc_core::ident::Identifier;
use mc_data::registry::RegistryId;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, DataResult, Either};

/// Uninhabited direct value of registries that only accept references (`RegistryFixedCodec`).
#[derive(Clone, Debug, PartialEq)]
pub enum NoDirect {}

#[derive(Clone, Debug, PartialEq)]
pub enum Holder<T> {
    /// A registered entry (`Holder.Reference`), by name.
    Reference(Identifier),
    /// An inline definition (`Holder.Direct`).
    Direct(T),
}

impl<T> Holder<T> {
    pub fn reference(name: &str) -> Holder<T> {
        Holder::Reference(Identifier::parse(name).expect("valid identifier"))
    }
    /// `unwrapKey()` for references.
    pub fn name(&self) -> Option<&Identifier> {
        match self {
            Holder::Reference(n) => Some(n),
            Holder::Direct(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HolderSet<T> {
    /// `HolderSet.Named`: a tag (without the `#`).
    Named(Identifier),
    /// `HolderSet.Direct`.
    Direct(Vec<Holder<T>>),
}

fn unknown_registry_key(registry: &str, name: &Identifier) -> String {
    format!("Unknown registry key in ResourceKey[minecraft:root / {registry}]: {name}")
}

/// `Registry.byNameCodec()` / `holderByNameCodec()` for a built-in typed id (`Item`, `Block`, `EntityType`, ...).
pub fn registry_by_name_codec<T: RegistryId>() -> Codec<T> {
    IDENTIFIER.flat_xmap(
        |name| match T::registry().id_of(&name.to_string()).and_then(T::from_id) {
            Some(t) => DataResult::success(t),
            None => {
                let m = unknown_registry_key(T::registry().name(), &name);
                DataResult::error(move || m.clone())
            }
        },
        |t: &T| {
            DataResult::success(
                Identifier::parse(T::registry().name_of(t.id()).expect("valid id"))
                    .expect("valid name"),
            )
        },
    )
}

/// A name that must exist in `registry` (built-in or data-driven): `holderByNameCodec` without the holder.
pub fn registry_name_codec(registry: &'static str) -> Codec<Identifier> {
    IDENTIFIER.flat_xmap(
        move |name| {
            if ctx::registry_contains(registry, &name) {
                DataResult::success(name)
            } else {
                let m = unknown_registry_key(registry, &name);
                DataResult::error(move || m.clone())
            }
        },
        DataResult::success_ref,
    )
}

/// `RegistryFixedCodec.create(key)`.
pub fn holder_fixed_codec<T: 'static>(registry: &'static str) -> Codec<Holder<T>> {
    Codec::from_fns(
        move |ops, input: &Holder<T>, prefix| match input {
            Holder::Reference(id) => IDENTIFIER.encode(ops, id, prefix),
            Holder::Direct(_) => DataResult::error(move || {
                format!("Elements from registry {registry} can't be serialized to a value")
            }),
        },
        move |ops, input| {
            IDENTIFIER.decode(ops, input).flat_map(|(id, rest)| {
                if ctx::registry_contains(registry, &id) {
                    DataResult::success((Holder::Reference(id), rest))
                } else {
                    DataResult::error(move || {
                        format!("Failed to get element {id} from registry {registry}")
                    })
                }
            })
        },
    )
}

/// `RegistryFileCodec.create(key, elementCodec, allowInline)`.
pub fn holder_file_codec<T: Clone + 'static>(
    registry: &'static str,
    element: &Codec<T>,
    allow_inline: bool,
) -> Codec<Holder<T>> {
    let (e, d) = (element.clone(), element.clone());
    Codec::from_fns(
        move |ops, input: &Holder<T>, prefix| match input {
            Holder::Reference(id) => IDENTIFIER.encode(ops, id, prefix),
            Holder::Direct(v) => e.encode(ops, v, prefix),
        },
        move |ops, input| match IDENTIFIER.decode(ops, input) {
            DataResult::Success((id, rest)) => {
                if ctx::registry_contains(registry, &id) {
                    DataResult::success((Holder::Reference(id), rest))
                } else {
                    DataResult::error(move || {
                        format!("Failed to get element {id} from registry {registry}")
                    })
                }
            }
            DataResult::Error { .. } => {
                if !allow_inline {
                    DataResult::error_str("Inline definitions not allowed here")
                } else {
                    d.decode(ops, input).map(|(v, r)| (Holder::Direct(v), r))
                }
            }
        },
    )
}

use super::common::hashed_tag_codec;

/// `HolderSetCodec.create(key, elementCodec, alwaysUseList)`.
pub fn holder_set_codec<T: Clone + Send + Sync + 'static>(
    registry: &'static str,
    element: &Codec<Holder<T>>,
    always_use_list: bool,
) -> Codec<HolderSet<T>> {
    let list = element.list_of();
    let direct = if always_use_list {
        list
    } else {
        mc_dfu::extra::compact_list_codec_with(element, &list)
    };
    let either = hashed_tag_codec().either(&direct);
    let (e1, e2) = (either.clone(), either);
    Codec::from_fns(
        move |ops, input: &HolderSet<T>, prefix| match input {
            HolderSet::Named(tag) => e1.encode(ops, &Either::Left(tag.clone()), prefix),
            HolderSet::Direct(v) => e1.encode(ops, &Either::Right(v.clone()), prefix),
        },
        move |ops, input| {
            e2.decode(ops, input)
                .flat_map(|(either, rest)| match either {
                    Either::Left(tag) => {
                        if ctx::tag_exists(registry, &tag) {
                            DataResult::success((HolderSet::Named(tag), rest))
                        } else {
                            DataResult::error(move || {
                                format!("Missing tag: '{tag}' in '{registry}'")
                            })
                        }
                    }
                    Either::Right(values) => DataResult::success((HolderSet::Direct(values), rest)),
                })
        },
    )
}

// ---- stream codecs ------------------------------------------------------------------------------------------

/// `Registry.getIdOrThrow` (`IllegalArgumentException` in vanilla, an `EncoderException` once the packet codec wraps it).
fn id_or_err(registry: &str, name: &Identifier) -> EncodeResult<i32> {
    ctx::registry_id(registry, name).ok_or_else(|| {
        crate::codec::EncodeError(format!("Can't find id for '{name}' in registry {registry}"))
    })
}

fn name_or_err(registry: &str, id: i32) -> DecodeResult<Identifier> {
    ctx::registry_name(registry, id)
        .ok_or_else(|| DecodeError::Message(format!("No value with id {id}")))
}

/// `ByteBufCodecs.registry(key)` for a reference by name: the plain VarInt id.
pub fn encode_registry_name(
    registry: &str,
    name: &Identifier,
    out: &mut Vec<u8>,
) -> EncodeResult<()> {
    write_varint(out, id_or_err(registry, name)?);
    Ok(())
}
pub fn decode_registry_name(registry: &str, input: &mut &[u8]) -> DecodeResult<Identifier> {
    let id = read_varint(input)?;
    name_or_err(registry, id)
}

/// `ByteBufCodecs.holderRegistry(key)`: VarInt id of a reference.
pub fn encode_holder_registry<T>(
    registry: &str,
    holder: &Holder<T>,
    out: &mut Vec<u8>,
) -> EncodeResult<()> {
    match holder {
        Holder::Reference(n) => encode_registry_name(registry, n, out),
        Holder::Direct(_) => Err(crate::codec::EncodeError(format!(
            "Can't serialize a direct holder through holderRegistry({registry})"
        ))),
    }
}
pub fn decode_holder_registry<T>(registry: &str, input: &mut &[u8]) -> DecodeResult<Holder<T>> {
    decode_registry_name(registry, input).map(Holder::Reference)
}

/// `ByteBufCodecs.holder(key, directCodec)`: `0` + inline value, or `id + 1`.
pub fn encode_holder<T>(
    registry: &str,
    holder: &Holder<T>,
    out: &mut Vec<u8>,
    direct: impl FnOnce(&T, &mut Vec<u8>) -> EncodeResult<()>,
) -> EncodeResult<()> {
    match holder {
        Holder::Reference(n) => write_varint(out, id_or_err(registry, n)?.wrapping_add(1)),
        Holder::Direct(v) => {
            write_varint(out, 0);
            direct(v, out)?;
        }
    }
    Ok(())
}
pub fn decode_holder<T>(
    registry: &str,
    input: &mut &[u8],
    direct: impl FnOnce(&mut &[u8]) -> DecodeResult<T>,
) -> DecodeResult<Holder<T>> {
    let id = read_varint(input)?;
    if id == 0 {
        direct(input).map(Holder::Direct)
    } else {
        name_or_err(registry, id.wrapping_sub(1)).map(Holder::Reference)
    }
}

/// `ByteBufCodecs.holderSet(key)`.
pub fn encode_holder_set<T>(
    registry: &str,
    set: &HolderSet<T>,
    out: &mut Vec<u8>,
) -> EncodeResult<()> {
    match set {
        HolderSet::Named(tag) => {
            write_varint(out, 0);
            tag.encode(out)?;
        }
        HolderSet::Direct(v) => {
            write_varint(out, v.len() as i32 + 1);
            for h in v {
                encode_holder_registry(registry, h, out)?;
            }
        }
    };
    Ok(())
}
pub fn decode_holder_set<T>(registry: &str, input: &mut &[u8]) -> DecodeResult<HolderSet<T>> {
    let count = i64::from(read_varint(input)?) - 1;
    if count == -1 {
        // `registry.get(TagKey).orElseThrow()`
        let tag = Identifier::decode(input)?;
        if !ctx::get().tag_exists(registry, &tag) {
            return Err(DecodeError::msg("No value present"));
        }
        return Ok(HolderSet::Named(tag));
    }
    if count < -1 {
        return Err(DecodeError::Message(format!("Illegal Capacity: {count}")));
    }
    let mut v = Vec::with_capacity((count as usize).min(65536));
    for _ in 0..count {
        v.push(decode_holder_registry(registry, input)?);
    }
    Ok(HolderSet::Direct(v))
}
