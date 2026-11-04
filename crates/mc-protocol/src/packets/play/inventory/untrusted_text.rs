//! `ComponentSerialization.STREAM_CODEC` / `OPTIONAL_STREAM_CODEC`: the same wire form as the trusted `Component` codec but
//! the NBT is read with the default 2 MiB accounter. For `#[mc(with = ..)]`.
use mc_text::Component;

use crate::codec::{DecodeError, DecodeResult, NbtLimit, decode_tag_with};
use crate::{Decode, Encode};

pub mod component {
    use super::*;

    pub fn encode(c: &Component, out: &mut Vec<u8>) {
        c.encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Component> {
        let tag = decode_tag_with(input, NbtLimit::DEFAULT)?
            .ok_or_else(|| DecodeError::msg("Expected non-null compound tag"))?;
        Component::from_nbt(&tag)
            .map_err(|e| DecodeError::Message(format!("Failed to decode: {e} {tag}")))
    }
}

pub mod optional_component {
    use super::*;

    pub fn encode(c: &Option<Component>, out: &mut Vec<u8>) {
        c.encode(out)
    }
    pub fn decode(input: &mut &[u8]) -> DecodeResult<Option<Component>> {
        if bool::decode(input)? {
            component::decode(input).map(Some)
        } else {
            Ok(None)
        }
    }
}
