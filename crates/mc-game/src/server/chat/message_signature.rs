//! `net.minecraft.network.chat.MessageSignature` helpers. The value type (256 raw bytes) and its codec are
//! `mc_protocol::codec::MessageSignature`; its `Packed` wire form is `MessageSignaturePacked`.

use super::message_signature_cache::MessageSignatureCache;
use base64::Engine;
pub use mc_protocol::codec::MessageSignature;
use mc_protocol::packets::play::MessageSignaturePacked;

pub const BYTES: usize = 256;

/// `MessageSignature.checksum()` / `Arrays.hashCode(bytes)` (bytes are signed in Java).
pub fn checksum(signature: &MessageSignature) -> i32 {
    signature.0.iter().fold(1i32, |h, &b| h.wrapping_mul(31).wrapping_add(i32::from(b as i8)))
}

/// `MessageSignature.toString()`: base64.
pub fn to_base64(signature: &MessageSignature) -> String {
    base64::engine::general_purpose::STANDARD.encode(signature.0)
}

/// `MessageSignature.describe`.
pub fn describe(signature: Option<&MessageSignature>) -> String {
    signature.map_or_else(|| "<no signature>".to_owned(), to_base64)
}

/// `MessageSignature.pack(cache)`.
pub fn pack(signature: &MessageSignature, cache: &MessageSignatureCache) -> MessageSignaturePacked {
    match cache.pack(signature) {
        -1 => MessageSignaturePacked::Full(signature.clone()),
        id => MessageSignaturePacked::Cached(id),
    }
}

/// `MessageSignature.Packed.unpack(cache)`: `Err` where vanilla's array access throws (id outside the cache).
pub fn unpack(packed: &MessageSignaturePacked, cache: &MessageSignatureCache) -> Result<Option<MessageSignature>, super::message_signature_cache::IdOutOfRange> {
    match packed {
        MessageSignaturePacked::Full(signature) => Ok(Some(signature.clone())),
        MessageSignaturePacked::Cached(id) => cache.unpack(*id),
    }
}
