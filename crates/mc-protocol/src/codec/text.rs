//! `Component` codecs: NBT (`ComponentSerialization.TRUSTED_CONTEXT_FREE_STREAM_CODEC`) and JSON text.
use mc_text::Component;

use super::{read_utf, write_utf, Decode, DecodeError, DecodeResult, Encode, NbtLimit};

/// Component as NBT in network form (text-only components are a bare string tag).
///
/// # Panics
/// On a component vanilla refuses to serialize (e.g. a failing validator or an over-long string).
impl Encode for Component {
    fn encode(&self, out: &mut Vec<u8>) {
        match self.to_nbt() {
            Ok(tag) => super::encode_tag(&tag, out),
            Err(e) => panic!("Failed to encode: {e} {self:?}"),
        }
    }
}
impl Decode for Component {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let tag = match super::decode_tag_with(input, NbtLimit::UNLIMITED)? {
            Some(t) => t,
            None => return Err(DecodeError::msg("Expected non-null compound tag")),
        };
        Component::from_nbt(&tag).map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {tag}")))
    }
}

/// Component as a JSON string (`lenientJson(262144)` + `ComponentSerialization.CODEC`), used by the
/// login disconnect packet.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonComponent(pub Component);

/// Vanilla limit of `ClientboundLoginDisconnectPacket`'s JSON string.
const JSON_MAX: usize = 262144;

impl Encode for JsonComponent {
    fn encode(&self, out: &mut Vec<u8>) {
        match self.0.to_json_string() {
            Ok(s) => write_utf(out, &s),
            Err(e) => panic!("Failed to encode: {e} {:?}", self.0),
        }
    }
}
impl Decode for JsonComponent {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let s = read_utf(input, JSON_MAX)?;
        let json: serde_json::Value =
            serde_json::from_str(&s).map_err(|e| DecodeError::Message(format!("Failed to parse JSON: {e}")))?;
        Component::from_json(&json)
            .map(Self)
            .map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {json}")))
    }
}
