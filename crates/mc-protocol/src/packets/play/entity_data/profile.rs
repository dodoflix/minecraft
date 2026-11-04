//! `ResolvableProfile.STREAM_CODEC` in wire form (the profile data component will share this type once `components`
//! provides it: swap the single `use` in `serializers.rs`).
use mc_core::ident::Identifier;
use uuid::Uuid;

use crate::codec::{
    DecodeBounded, DecodeResult, Either, GameProfile, Property, read_count, write_varint,
};
use crate::{Decode, Encode};

/// `PlayerModelType.STREAM_CODEC`: a bool, `true` = slim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerModelType {
    Slim,
    Wide,
}

impl Encode for PlayerModelType {
    fn encode(&self, out: &mut Vec<u8>) {
        (*self == Self::Slim).encode(out)
    }
}
impl Decode for PlayerModelType {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        bool::decode(input).map(|slim| if slim { Self::Slim } else { Self::Wide })
    }
}

/// `PlayerSkin.Patch.STREAM_CODEC`: optional texture / cape / elytra identifiers and model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
pub struct PlayerSkinPatch {
    pub body: Option<Identifier>,
    pub cape: Option<Identifier>,
    pub elytra: Option<Identifier>,
    pub model: Option<PlayerModelType>,
}

/// `ByteBufCodecs.GAME_PROFILE_PROPERTIES`: at most 16 properties (name <= 64, value <= 32767, signature <= 1024).
mod properties {
    use super::*;

    pub fn encode(props: &Vec<Property>, out: &mut Vec<u8>) {
        write_varint(out, props.len() as i32);
        for p in props {
            p.name.encode(out);
            p.value.encode(out);
            p.signature.encode(out);
        }
    }

    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<Property>> {
        let count = read_count(input, 16)?;
        let mut props = Vec::with_capacity(count);
        for _ in 0..count {
            props.push(Property {
                name: String::decode_bounded(input, 64)?,
                value: String::decode_bounded(input, 32767)?,
                signature: Option::<String>::decode_bounded(input, 1024)?,
            });
        }
        Ok(props)
    }
}

/// `ResolvableProfile.Partial.STREAM_CODEC`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResolvableProfilePartial {
    #[mc(max = 16)]
    pub name: Option<String>,
    pub id: Option<Uuid>,
    #[mc(with = properties)]
    pub properties: Vec<Property>,
}

/// `ResolvableProfile.STREAM_CODEC`: a full `GameProfile` (`Static`) or a partial one (`Static`/`Dynamic`), plus the skin patch.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResolvableProfile {
    pub contents: Either<GameProfile, ResolvableProfilePartial>,
    pub skin_patch: PlayerSkinPatch,
}
