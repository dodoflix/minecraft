//! Packets shared by the configuration and play states (vanilla `network.protocol.common` and
//! `network.protocol.cookie`), plus their helper types.
use crate::codec::EncodeResult;
use mc_core::ident::Identifier;
use mc_nbt::Tag;
use mc_text::Component;

use crate::codec::{
    read_count, read_utf, write_varint, DecodeBounded, DecodeResult, Either, LengthPrefixed, Rest, VarEncode,
};
use crate::{Decode, Encode};

/// `ClientInformation` (see `ServerboundClientInformationPacket`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInformation {
    pub language: String,
    pub view_distance: i8,
    pub chat_visibility: ChatVisibility,
    pub chat_colors: bool,
    /// Unsigned byte on the wire.
    pub model_customisation: u8,
    pub main_hand: HumanoidArm,
    pub text_filtering_enabled: bool,
    pub allows_listing: bool,
    pub particle_status: ParticleStatus,
}

macro_rules! id_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $id:literal),+ } out_of_bounds = $oob:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }
        impl $name {
            const VALUES: &[Self] = &[$(Self::$variant),+];
            pub fn id(self) -> i32 { match self { $(Self::$variant => $id),+ } }
            /// `ByIdMap.continuous` lookup with vanilla's out-of-bounds strategy.
            pub fn by_id(id: i32) -> Self {
                let n = Self::VALUES.len() as i32;
                id_enum!(@lookup $oob, id, n, Self::VALUES)
            }
        }
        impl $crate::codec::Encode for $name {
            fn encode(&self, out: &mut Vec<u8>) -> $crate::codec::EncodeResult<()> { $crate::codec::write_varint(out, self.id()); Ok(()) }
        }
        impl $crate::codec::Decode for $name {
            fn decode(input: &mut &[u8]) -> $crate::codec::DecodeResult<Self> { $crate::codec::read_varint(input).map(Self::by_id) }
        }
    };
    (@lookup WRAP, $id:ident, $n:ident, $values:expr) => { $values[$id.rem_euclid($n) as usize] };
    (@lookup ZERO, $id:ident, $n:ident, $values:expr) => { $values[if (0..$n).contains(&$id) { $id as usize } else { 0 }] };
}

id_enum!(
    /// `ChatVisiblity`
    ChatVisibility { Full = 0, System = 1, Hidden = 2 } out_of_bounds = WRAP
);
id_enum!(
    /// `HumanoidArm`
    HumanoidArm { Left = 0, Right = 1 } out_of_bounds = ZERO
);
id_enum!(
    /// `ParticleStatus`
    ParticleStatus { All = 0, Decreased = 1, Minimal = 2 } out_of_bounds = WRAP
);

impl Default for ClientInformation {
    /// `ClientInformation.createDefault()`
    fn default() -> Self {
        Self {
            language: "en_us".into(),
            view_distance: 2,
            chat_visibility: ChatVisibility::Full,
            chat_colors: true,
            model_customisation: 0,
            main_hand: HumanoidArm::Right,
            text_filtering_enabled: false,
            allows_listing: false,
            particle_status: ParticleStatus::All,
        }
    }
}

impl Encode for ClientInformation {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.language.encode(out)?;
        self.view_distance.encode(out)?;
        self.chat_visibility.encode(out)?;
        self.chat_colors.encode(out)?;
        self.model_customisation.encode(out)?;
        self.main_hand.encode(out)?;
        self.text_filtering_enabled.encode(out)?;
        self.allows_listing.encode(out)?;
        self.particle_status.encode(out)?;
        Ok(())
    }
}
impl Decode for ClientInformation {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self {
            language: String::decode_bounded(input, 16)?,
            view_distance: i8::decode(input)?,
            chat_visibility: ChatVisibility::decode(input)?,
            chat_colors: bool::decode(input)?,
            model_customisation: u8::decode(input)?,
            main_hand: HumanoidArm::decode(input)?,
            text_filtering_enabled: bool::decode(input)?,
            allows_listing: bool::decode(input)?,
            particle_status: ParticleStatus::decode(input)?,
        })
    }
}

/// `CustomPacketPayload` as seen by a server that registers only `minecraft:brand`: the channel id
/// plus the raw payload bytes (for `minecraft:brand`, the encoded brand string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomPayload<const MAX: usize> {
    pub id: Identifier,
    pub data: Vec<u8>,
}

impl<const MAX: usize> CustomPayload<MAX> {
    /// `BrandPayload`
    pub fn brand(brand: &str) -> crate::codec::EncodeResult<Self> {
        Ok(Self { id: Identifier::with_default_namespace("brand").expect("valid"), data: crate::codec::to_bytes(brand)? })
    }

    /// The brand string when this is a `minecraft:brand` payload.
    pub fn as_brand(&self) -> Option<String> {
        (self.id == Identifier::with_default_namespace("brand").expect("valid"))
            .then(|| crate::codec::from_bytes(&self.data).ok())
            .flatten()
    }
}

impl<const MAX: usize> Encode for CustomPayload<MAX> {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.id.encode(out)?;
        out.extend_from_slice(&self.data);
        Ok(())
    }
}
impl<const MAX: usize> Decode for CustomPayload<MAX> {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let id = Identifier::decode(input)?;
        if id.namespace() == "minecraft" && id.path() == "brand" {
            // BrandPayload reads exactly one string; any trailing bytes make the packet "larger than expected".
            let before = *input;
            let _brand = String::decode(input)?;
            let used = before.len() - input.len();
            return Ok(Self { id, data: before[..used].to_vec() });
        }
        Ok(Self { id, data: Rest::<MAX>::decode(input)?.0 })
    }
}

/// `ServerLinks.KnownLinkType`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnownLinkType {
    BugReport = 0,
    CommunityGuidelines,
    Support,
    Status,
    Feedback,
    Community,
    Website,
    Forums,
    News,
    Announcements,
}

impl KnownLinkType {
    const VALUES: [Self; 10] = [
        Self::BugReport,
        Self::CommunityGuidelines,
        Self::Support,
        Self::Status,
        Self::Feedback,
        Self::Community,
        Self::Website,
        Self::Forums,
        Self::News,
        Self::Announcements,
    ];
}
impl Encode for KnownLinkType {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, *self as i32);
        Ok(())
    }
}
impl Decode for KnownLinkType {
    /// `ByIdMap.OutOfBoundsStrategy.ZERO`
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let id = crate::codec::read_varint(input)?;
        Ok(usize::try_from(id).ok().and_then(|i| Self::VALUES.get(i)).copied().unwrap_or(Self::BugReport))
    }
}

/// `ServerLinks.UntrustedEntry`
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct UntrustedEntry {
    pub link_type: Either<KnownLinkType, Component>,
    pub link: String,
}

/// `TagNetworkSerialization.NetworkPayload`: tag name -> registry ids.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagNetworkPayload {
    pub tags: Vec<(Identifier, Vec<i32>)>,
}

impl Encode for TagNetworkPayload {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        write_varint(out, self.tags.len() as i32);
        for (name, ids) in &self.tags {
            name.encode(out)?;
            ids.encode_var(out);
        };
        Ok(())
    }
}
impl Decode for TagNetworkPayload {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, i32::MAX as usize)?;
        let mut tags = Vec::with_capacity(count.min(65536));
        for _ in 0..count {
            tags.push((Identifier::decode(input)?, Vec::<i32>::decode_var(input)?));
        }
        Ok(Self { tags })
    }
}

/// `ServerboundResourcePackPacket.Action` (VarInt ordinal; out-of-range ordinals fail like Java's array access).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
#[mc(error = "Index {} out of bounds for length 8")]
pub enum ResourcePackAction {
    SuccessfullyLoaded,
    Declined,
    FailedDownload,
    Accepted,
    Downloaded,
    InvalidUrl,
    FailedReload,
    Discarded,
}

impl ResourcePackAction {
    /// Final states: the server may stop waiting for this pack.
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Accepted | Self::Downloaded)
    }
}

/// `ClientboundCustomReportDetailsPacket` details: at most 32 entries, key <= 128, value <= 4096 chars.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReportDetails(pub Vec<(String, String)>);

impl Encode for ReportDetails {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        self.0.encode(out)?;
        Ok(())
    }
}
impl Decode for ReportDetails {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let count = read_count(input, 32)?;
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push((read_utf(input, 128)?, read_utf(input, 4096)?));
        }
        Ok(Self(v))
    }
}

/// `ServerboundCustomClickActionPacket` payload: optional tag (NBT `End` = none), length-prefixed
/// (max 65536 bytes), read with an `NbtAccounter(32768, 16)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickPayload(pub Option<Tag>);

impl Encode for ClickPayload {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        let mut inner = Vec::new();
        match &self.0 {
            Some(t) => crate::codec::encode_tag(t, &mut inner)?,
            None => inner.push(0),
        }
        LengthPrefixed::<Raw, 65536>(Raw(inner)).encode(out)?;
        Ok(())
    }
}
impl Decode for ClickPayload {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        let Raw(inner) = LengthPrefixed::<Raw, 65536>::decode(input)?.0;
        crate::codec::decode_tag_with(&mut &inner[..], crate::codec::NbtLimit::new(32768, 16)).map(Self)
    }
}

/// All remaining bytes of the (sub-)buffer.
struct Raw(Vec<u8>);
impl Encode for Raw {
    fn encode(&self, out: &mut Vec<u8>) -> EncodeResult<()> {
        out.extend_from_slice(&self.0);
        Ok(())
    }
}
impl Decode for Raw {
    fn decode(input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self(std::mem::take(input).to_vec()))
    }
}

/// Declares the packets of `common` (vanilla `network.protocol.common`) for one state.
macro_rules! common_packets {
    ($state:ident) => {
        /// `ClientboundClearDialogPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "clear_dialog")]
        pub struct ClientboundClearDialogPacket;

        /// `ClientboundCustomPayloadPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "custom_payload")]
        pub struct ClientboundCustomPayloadPacket {
            pub payload: $crate::packets::common::CustomPayload<1048576>,
        }

        /// `ClientboundCustomReportDetailsPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "custom_report_details")]
        pub struct ClientboundCustomReportDetailsPacket {
            pub details: $crate::packets::common::ReportDetails,
        }

        /// `ClientboundDisconnectPacket`
        #[derive(Debug, Clone, PartialEq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "disconnect")]
        pub struct ClientboundDisconnectPacket {
            pub reason: ::mc_text::Component,
        }

        /// `ClientboundKeepAlivePacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "keep_alive")]
        pub struct ClientboundKeepAlivePacket {
            pub id: i64,
        }

        /// `ClientboundPingPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "ping")]
        pub struct ClientboundPingPacket {
            pub id: i32,
        }

        /// `ClientboundPostEffectsPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "post_effects")]
        pub struct ClientboundPostEffectsPacket {
            pub post_effects: Vec<::mc_core::ident::Identifier>,
        }

        /// `ClientboundResourcePackPopPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "resource_pack_pop")]
        pub struct ClientboundResourcePackPopPacket {
            pub id: Option<::uuid::Uuid>,
        }

        /// `ClientboundResourcePackPushPacket`
        #[derive(Debug, Clone, PartialEq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "resource_pack_push")]
        pub struct ClientboundResourcePackPushPacket {
            pub id: ::uuid::Uuid,
            pub url: String,
            #[mc(max = 40)]
            pub hash: String,
            pub required: bool,
            pub prompt: Option<::mc_text::Component>,
        }

        /// `ClientboundServerLinksPacket`
        #[derive(Debug, Clone, PartialEq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "server_links")]
        pub struct ClientboundServerLinksPacket {
            pub links: Vec<$crate::packets::common::UntrustedEntry>,
        }

        /// `ClientboundStoreCookiePacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "store_cookie")]
        pub struct ClientboundStoreCookiePacket {
            pub key: ::mc_core::ident::Identifier,
            #[mc(max = 5120)]
            pub payload: Vec<u8>,
        }

        /// `ClientboundTransferPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "transfer")]
        pub struct ClientboundTransferPacket {
            pub host: String,
            #[mc(var)]
            pub port: i32,
        }

        /// `ClientboundUpdateTagsPacket`: registry key -> tags.
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "update_tags")]
        pub struct ClientboundUpdateTagsPacket {
            pub tags: Vec<(::mc_core::ident::Identifier, $crate::packets::common::TagNetworkPayload)>,
        }

        /// `ServerboundClientInformationPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = serverbound, name = "client_information")]
        pub struct ServerboundClientInformationPacket {
            pub information: $crate::packets::common::ClientInformation,
        }

        /// `ServerboundCustomClickActionPacket`
        #[derive(Debug, Clone, PartialEq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = serverbound, name = "custom_click_action")]
        pub struct ServerboundCustomClickActionPacket {
            pub id: ::mc_core::ident::Identifier,
            pub payload: $crate::packets::common::ClickPayload,
        }

        /// `ServerboundCustomPayloadPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = serverbound, name = "custom_payload")]
        pub struct ServerboundCustomPayloadPacket {
            pub payload: $crate::packets::common::CustomPayload<32767>,
        }

        /// `ServerboundKeepAlivePacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = serverbound, name = "keep_alive")]
        pub struct ServerboundKeepAlivePacket {
            pub id: i64,
        }

        /// `ServerboundPongPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = serverbound, name = "pong")]
        pub struct ServerboundPongPacket {
            pub id: i32,
        }

        /// `ServerboundResourcePackPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = serverbound, name = "resource_pack")]
        pub struct ServerboundResourcePackPacket {
            pub id: ::uuid::Uuid,
            pub action: $crate::packets::common::ResourcePackAction,
        }
    };
}

/// Declares the cookie packets (vanilla `network.protocol.cookie`) for one state.
macro_rules! cookie_packets {
    ($state:ident) => {
        /// `ClientboundCookieRequestPacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = clientbound, name = "cookie_request")]
        pub struct ClientboundCookieRequestPacket {
            pub key: ::mc_core::ident::Identifier,
        }

        /// `ServerboundCookieResponsePacket`
        #[derive(Debug, Clone, PartialEq, Eq, $crate::Encode, $crate::Decode, $crate::Packet)]
        #[packet(state = $state, flow = serverbound, name = "cookie_response")]
        pub struct ServerboundCookieResponsePacket {
            pub key: ::mc_core::ident::Identifier,
            #[mc(max = 5120)]
            pub payload: Option<Vec<u8>>,
        }
    };
}
pub(crate) use {common_packets, cookie_packets, id_enum};
