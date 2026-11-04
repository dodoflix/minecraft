//! Wire codec: the Rust form of vanilla's `FriendlyByteBuf` / `ByteBufCodecs`.
//!
//! Encoding appends to a `Vec<u8>` and cannot fail; decoding consumes a `&mut &[u8]` and returns
//! [`DecodeError`] whose messages follow the vanilla `DecoderException`s. Limits that vanilla checks
//! on *write* (string/array maxima) are the caller's responsibility; every limit vanilla checks on
//! *read* is enforced by `Decode`.

mod bits;
mod misc;
mod nbt;
mod prim;
mod registry;
mod text;
mod types;
mod varint;

pub use bits::{BitSet, BitSetBytes, EnumSet, EnumValues, FixedBitSet};
pub use misc::*;
pub use nbt::{decode_tag, decode_tag_with, encode_tag, NbtLimit};
pub use prim::{read_utf, utf16_len, write_utf, LongArray, MAX_STRING_LENGTH};
pub use registry::read_registry_id;
pub use text::JsonComponent;
pub use types::*;
pub use varint::{read_varint, read_varlong, varint_size, varlong_size, write_varint, write_varlong, VarInt, VarLong};

/// Error while decoding. `Message` carries the text of the vanilla `DecoderException`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// Netty `IndexOutOfBoundsException` on reading past the end of the frame.
    #[error("Not enough bytes in buffer: needed {needed}, but only {available} readable")]
    Eof { needed: usize, available: usize },
    #[error("VarInt too big")]
    VarIntTooBig,
    #[error("VarLong too big")]
    VarLongTooBig,
    #[error("{0}")]
    Message(String),
}

impl DecodeError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

pub type DecodeResult<T> = Result<T, DecodeError>;

pub trait Encode {
    fn encode(&self, out: &mut Vec<u8>);

    #[doc(hidden)]
    fn encode_slice(items: &[Self], out: &mut Vec<u8>)
    where
        Self: Sized,
    {
        for i in items {
            i.encode(out);
        }
    }
}

pub trait Decode: Sized {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self>;

    /// `ByteBufCodecs.collection`: VarInt count (rejected above `max`), then the elements.
    #[doc(hidden)]
    fn decode_vec(input: &mut &[u8], max: usize) -> DecodeResult<Vec<Self>> {
        let count = read_count(input, max)?;
        let mut v = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            v.push(Self::decode(input)?);
        }
        Ok(v)
    }
}

/// Decoding with a vanilla-style maximum (string length in UTF-16 units, collection element count).
pub trait DecodeBounded: Sized {
    fn decode_bounded(input: &mut &[u8], max: usize) -> DecodeResult<Self>;
}

/// `VarInt`/`VarLong` encoding of the plain integer types (selected by `#[mc(var)]`).
pub trait VarEncode: Sized {
    fn encode_var(&self, out: &mut Vec<u8>);
    fn decode_var(input: &mut &[u8]) -> DecodeResult<Self>;
}

impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out)
    }
}

impl<T: Encode + ?Sized> Encode for Box<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out)
    }
}

impl<T: Decode> Decode for Box<T> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        T::decode(input).map(Box::new)
    }
}

/// Consume exactly `n` bytes.
#[inline]
pub fn take<'a>(input: &mut &'a [u8], n: usize) -> DecodeResult<&'a [u8]> {
    if input.len() < n {
        return Err(DecodeError::Eof { needed: n, available: input.len() });
    }
    let (head, tail) = input.split_at(n);
    *input = tail;
    Ok(head)
}

#[inline]
pub fn take_array<const N: usize>(input: &mut &[u8]) -> DecodeResult<[u8; N]> {
    let s = take(input, N)?;
    Ok(s.try_into().expect("length checked"))
}

/// `ByteBufCodecs.readCount`: VarInt element count, rejected when above `max`.
/// A negative count passes vanilla's check but fails allocation (`Illegal Capacity`).
pub fn read_count(input: &mut &[u8], max: usize) -> DecodeResult<usize> {
    let count = read_varint(input)?;
    if count >= 0 && count as usize > max {
        return Err(DecodeError::Message(format!("{count} elements exceeded max size of: {max}")));
    }
    usize::try_from(count).map_err(|_| DecodeError::Message(format!("Illegal Capacity: {count}")))
}

/// Encode a value into a fresh buffer.
pub fn to_bytes<T: Encode + ?Sized>(v: &T) -> Vec<u8> {
    let mut out = Vec::new();
    v.encode(&mut out);
    out
}

/// Decode a value from `bytes`, requiring that all bytes are consumed.
pub fn from_bytes<T: Decode>(mut bytes: &[u8]) -> DecodeResult<T> {
    let v = T::decode(&mut bytes)?;
    if !bytes.is_empty() {
        return Err(DecodeError::Message(format!("{} extra bytes after decoding", bytes.len())));
    }
    Ok(v)
}
