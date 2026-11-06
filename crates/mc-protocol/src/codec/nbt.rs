//! NBT on the wire (`FriendlyByteBuf.writeNbt` / `ByteBufCodecs.TAG`): network form, unnamed root.
use mc_nbt::{CompoundTag, NbtAccounter, Tag};

use super::{Decode, DecodeError, DecodeResult, Encode, EncodeError, EncodeResult};

/// Quota + depth limit of an `NbtAccounter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NbtLimit {
    pub quota: i64,
    pub depth: i32,
}

impl NbtLimit {
    /// `NbtAccounter.defaultQuota()`: 2 MiB, depth 512.
    pub const DEFAULT: Self = Self {
        quota: 2_097_152,
        depth: 512,
    };
    /// `NbtAccounter.unlimitedHeap()`: used for trusted (server -> client) data.
    pub const UNLIMITED: Self = Self {
        quota: i64::MAX,
        depth: 512,
    };

    pub const fn new(quota: i64, depth: i32) -> Self {
        Self { quota, depth }
    }
}

/// `FriendlyByteBuf.writeNbt`: append `tag` in network form. `Tag::End` writes the single byte `0` (vanilla's
/// `null` tag). Fails like the `IOException` -> `EncoderException` wrapper: a string longer than 65535
/// modified-UTF-8 bytes (`UTFDataFormatException`). Vanilla has no write-side NBT quota or depth limit.
pub fn encode_tag(tag: &Tag, out: &mut Vec<u8>) -> EncodeResult<()> {
    mc_nbt::write_network(tag, out).map_err(|e| {
        // `new EncoderException(IOException)`: message = `cause.toString()`.
        EncodeError(match e {
            mc_nbt::NbtError::MalformedUtf(m) => format!("java.io.UTFDataFormatException: {m}"),
            other => other.to_string(),
        })
    })
}

/// `FriendlyByteBuf.readNbt(buf, accounter)`: `None` for a bare `End`.
pub fn decode_tag_with(input: &mut &[u8], limit: NbtLimit) -> DecodeResult<Option<Tag>> {
    let mut acc = NbtAccounter::new(limit.quota, limit.depth);
    match mc_nbt::read_network(input, &mut acc) {
        Ok(Tag::End) => Ok(None),
        Ok(t) => Ok(Some(t)),
        Err(e) => Err(DecodeError::Message(e.to_string())),
    }
}

/// `ByteBufCodecs.TAG` decoding: `End` is rejected.
pub fn decode_tag(input: &mut &[u8]) -> DecodeResult<Tag> {
    decode_tag_with(input, NbtLimit::DEFAULT)?
        .ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))
}

/// `ByteBufCodecs.TAG` encode: `End` is rejected.
impl Encode for Tag {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        if matches!(self, Tag::End) {
            return Err(EncodeError::msg("Expected non-null compound tag"));
        }
        encode_tag(self, out)
    }
}
impl Decode for Tag {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        decode_tag(input)
    }
}

/// `ByteBufCodecs.COMPOUND_TAG`
impl Encode for CompoundTag {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        encode_tag(&Tag::Compound(self.clone()), out)
    }
}
impl Decode for CompoundTag {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        match decode_tag(input)? {
            Tag::Compound(c) => Ok(c),
            other => Err(DecodeError::Message(format!("Not a compound tag: {other}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{from_bytes, to_bytes};

    #[test]
    fn network_form_has_no_root_name() {
        let mut c = CompoundTag::new();
        c.put_int("a", 1);
        let b = to_bytes(&Tag::Compound(c.clone())).unwrap();
        // TAG_Compound(10), TAG_Int(3) "a" 0 0 0 1, TAG_End
        assert_eq!(b, [10, 3, 0, 1, b'a', 0, 0, 0, 1, 0]);
        assert_eq!(from_bytes::<CompoundTag>(&b).unwrap(), c);
        assert_eq!(
            decode_tag_with(&mut &[0u8][..], NbtLimit::DEFAULT),
            Ok(None)
        );
        assert_eq!(
            from_bytes::<Tag>(&[0]),
            Err(DecodeError::msg("Expected non-null compound tag"))
        );
    }
}
