//! `profile` component: `ResolvableProfile` (`CODEC`, `STREAM_CODEC`) with `PlayerSkin.Patch` and the `GameProfile` /
//! `PropertyMap` codecs (`ExtraCodecs.STORED_GAME_PROFILE`, `PLAYER_NAME`, `PROPERTY_MAP`, `ByteBufCodecs.GAME_PROFILE`).
//!
//! Vanilla distinguishes `Static` and `Dynamic` profiles; `ResolvableProfile.create` makes that split canonical (a
//! partial with exactly one of name/id and no properties is `Dynamic`), so `contents` + `skin_patch` carry the same
//! information and the same equality.
use crate::codec::EncodeResult;
use crate::components::common::uuid_codec;
use crate::components::component::{ComponentDef, ComponentKey};
use crate::codec::{group_properties_by_name, read_property_count, DecodeBounded, DecodeResult, Either, GameProfile, Property};
use crate::string_enum;
use crate::{Decode, Encode};
use mc_core::ident::Identifier;
use mc_data::DataComponentType;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{Codec, DataResult, MapCodec, STRING};
use uuid::Uuid;

pub const PROFILE: ComponentKey<ResolvableProfile> = ComponentKey::new(DataComponentType::PROFILE);

string_enum! {
    /// `PlayerModelType`.
    pub enum PlayerModelType { Slim = "slim", Wide = "wide" }
}

/// `PlayerModelType.STREAM_CODEC`: a bool, `true` = slim.
impl Encode for PlayerModelType {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        (*self == Self::Slim).encode(out)?;
        Ok(())
    }
}
impl Decode for PlayerModelType {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        bool::decode(input).map(|slim| if slim { Self::Slim } else { Self::Wide })
    }
}

/// `PlayerSkin.Patch`: optional texture / cape / elytra identifiers and model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
pub struct PlayerSkinPatch {
    pub body: Option<Identifier>,
    pub cape: Option<Identifier>,
    pub elytra: Option<Identifier>,
    pub model: Option<PlayerModelType>,
}

impl PlayerSkinPatch {
    /// `PlayerSkin.Patch.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<PlayerSkinPatch> {
        mc_dfu::record!(PlayerSkinPatch {
            body: IDENTIFIER.optional_field_of("texture"),
            cape: IDENTIFIER.optional_field_of("cape"),
            elytra: IDENTIFIER.optional_field_of("elytra"),
            model: PlayerModelType::codec().optional_field_of("model"),
        })
    }
}

/// `ByteBufCodecs.GAME_PROFILE_PROPERTIES`: at most 16 properties (name <= 64, value <= 32767, signature <= 1024).
/// Properties live in a `Multimap`, so they are grouped by name in order of first appearance.
mod properties {
    use super::*;

    pub fn encode(props: &Vec<Property>, out: &mut Vec<u8>) -> EncodeResult<()> {
        crate::codec::write_properties(out, props)
    }

    pub fn decode(input: &mut &[u8]) -> DecodeResult<Vec<Property>> {
        let count = read_property_count(input)?;
        let mut props = Vec::with_capacity(count);
        for _ in 0..count {
            props.push(Property {
                name: String::decode_bounded(input, 64)?,
                value: String::decode_bounded(input, 32767)?,
                signature: Option::<String>::decode_bounded(input, 1024)?,
            });
        }
        Ok(group_properties_by_name(props))
    }
}

/// `ResolvableProfile.Partial`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResolvableProfilePartial {
    #[mc(max = 16)]
    pub name: Option<String>,
    pub id: Option<Uuid>,
    #[mc(with = properties)]
    pub properties: Vec<Property>,
}

/// `ExtraCodecs.PLAYER_NAME`.
pub fn player_name_codec() -> Codec<String> {
    mc_dfu::string_range(0, 16).validate(|name| {
        if name.encode_utf16().all(|c| c > 32 && c < 127) {
            DataResult::success(name)
        } else {
            DataResult::error(move || format!("Player name contained disallowed characters: '{name}'"))
        }
    })
}

/// `ExtraCodecs.PROPERTY_MAP`.
pub fn property_map_codec() -> Codec<Vec<Property>> {
    let property = mc_dfu::record!(Property {
        name: mc_dfu::size_limited_string(64).field_of("name"),
        value: mc_dfu::size_limited_string(32767).field_of("value"),
        signature: mc_dfu::size_limited_string(1024).optional_field_of("signature"),
    })
    .codec();
    let map = STRING.unbounded_map(&STRING.list_of()).validate(|map| {
        if map.len() > 16 {
            let n = map.len();
            DataResult::error(move || format!("Cannot have more than 16 properties, but was {n}"))
        } else {
            DataResult::success(map)
        }
    });
    map.either(&property.size_limited_list_of(16)).xmap(
        |e| {
            group_properties_by_name(match e {
                mc_dfu::Either::Left(map) => map
                    .into_iter()
                    .flat_map(|(name, values)| {
                        values.into_iter().map(move |value| Property { name: name.clone(), value, signature: None })
                    })
                    .collect(),
                mc_dfu::Either::Right(list) => list,
            })
        },
        |props: &Vec<Property>| mc_dfu::Either::Right(props.clone()),
    )
}

/// `ExtraCodecs.STORED_GAME_PROFILE`.
fn stored_game_profile_codec() -> MapCodec<GameProfile> {
    mc_dfu::record!(GameProfile {
        id: uuid_codec().field_of("id"),
        name: player_name_codec().field_of("name"),
        properties: property_map_codec().optional_field_of_default("properties", Vec::new()),
    })
}

impl ResolvableProfilePartial {
    /// `ResolvableProfile.Partial.MAP_CODEC`.
    pub fn map_codec() -> MapCodec<ResolvableProfilePartial> {
        mc_dfu::record!(ResolvableProfilePartial {
            name: player_name_codec().optional_field_of("name"),
            id: uuid_codec().optional_field_of("id"),
            properties: property_map_codec().optional_field_of_default("properties", Vec::new()),
        })
    }
}

/// A full `GameProfile` (`Static`) or a partial one (`Static`/`Dynamic`), plus the skin patch.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct ResolvableProfile {
    pub contents: Either<GameProfile, ResolvableProfilePartial>,
    pub skin_patch: PlayerSkinPatch,
}

/// `ResolvableProfile.STREAM_CODEC`.
impl Decode for ResolvableProfile {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(ResolvableProfile { contents: Decode::decode(input)?, skin_patch: PlayerSkinPatch::decode(input)? })
    }
}

impl ResolvableProfile {
    /// `ResolvableProfile.createUnresolved(name)`.
    pub fn create_unresolved(name: impl Into<String>) -> ResolvableProfile {
        ResolvableProfile {
            contents: Either::Right(ResolvableProfilePartial { name: Some(name.into()), id: None, properties: Vec::new() }),
            skin_patch: PlayerSkinPatch::default(),
        }
    }

    /// `ResolvableProfile.CODEC`.
    pub fn codec() -> Codec<ResolvableProfile> {
        let contents = stored_game_profile_codec().either(&ResolvableProfilePartial::map_codec()).xmap(
            |e| match e {
                mc_dfu::Either::Left(p) => Either::Left(p),
                mc_dfu::Either::Right(p) => Either::Right(p),
            },
            |c: &Either<GameProfile, ResolvableProfilePartial>| match c {
                Either::Left(p) => mc_dfu::Either::Left(p.clone()),
                Either::Right(p) => mc_dfu::Either::Right(p.clone()),
            },
        );
        let full = mc_dfu::record!(ResolvableProfile { contents: contents, skin_patch: PlayerSkinPatch::map_codec() }).codec();
        full.with_alternative_conv(&player_name_codec(), ResolvableProfile::create_unresolved)
    }
}

pub(crate) fn defs(out: &mut Vec<ComponentDef>) {
    out.push(ComponentDef::builder(PROFILE).persistent(ResolvableProfile::codec()).network_synchronized().build());
}
