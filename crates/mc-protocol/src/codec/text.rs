//! `Component` codecs: NBT (`ComponentSerialization.TRUSTED_CONTEXT_FREE_STREAM_CODEC`) and JSON text.
use mc_text::Component;

use super::{
    Decode, DecodeError, DecodeResult, Encode, EncodeError, EncodeResult, NbtLimit, read_utf,
    write_utf,
};

/// Component as NBT in network form (text-only components are a bare string tag). Fails like
/// `codec.encodeStart(..).getOrThrow(EncoderException)` for a component vanilla refuses to serialize
/// (e.g. a failing validator or an over-long string).
impl Encode for Component {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        let tag = self
            .to_nbt()
            .map_err(|e| EncodeError(format!("Failed to encode: {e} {self:?}")))?;
        super::encode_tag(&tag, out)
    }
}
impl Decode for Component {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let tag = match super::decode_tag_with(input, NbtLimit::UNLIMITED)? {
            Some(t) => t,
            None => return Err(DecodeError::msg("Expected non-null compound tag")),
        };
        Component::from_nbt(&tag)
            .map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {tag}")))
    }
}

/// `ComponentSerialization.TRUSTED_CONTEXT_FREE_STREAM_CODEC` as a field codec (`#[mc(with = ...)]`) for a `Component` or
/// an `Option<Component>`: the component is written by an `NbtOps` without registry access, so a dialog reference in a
/// click event is written as the dialog itself. Used by the disconnect reason, the resource pack prompt, the server data
/// motd and the display name of a server link; every other component of the protocol uses the registry-aware codec.
pub mod context_free {
    use super::{Decode, DecodeResult, Encode, EncodeResult};

    pub fn encode<T: Encode>(value: &T, out: &mut Vec<u8>) -> EncodeResult<()> {
        mc_text::with_context_free(|| value.encode(out))
    }

    pub fn decode<T: Decode>(input: &mut &[u8]) -> DecodeResult<T> {
        T::decode(input)
    }
}

/// Component as a JSON string (`lenientJson(262144)` + `ComponentSerialization.CODEC`), used by the
/// login disconnect packet.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonComponent(pub Component);

/// Vanilla limit of `ClientboundLoginDisconnectPacket`'s JSON string.
const JSON_MAX: usize = 262144;

impl Encode for JsonComponent {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        let s = self
            .0
            .to_json_string()
            .map_err(|e| EncodeError(format!("Failed to encode: {e} {:?}", self.0)))?;
        write_utf(out, &s, JSON_MAX)
    }
}
impl Decode for JsonComponent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let s = read_utf(input, JSON_MAX)?;
        let json: serde_json::Value = serde_json::from_str(&s)
            .map_err(|e| DecodeError::Message(format!("Failed to parse JSON: {e}")))?;
        Component::from_json(&json)
            .map(Self)
            .map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {json}")))
    }
}
