//! `play` state: all 144 clientbound / 69 serverbound packets of vanilla 26.3 (`GameProtocols`).
//!
//! The packets are split over group modules (re-exported here, so every packet is `packets::play::<VanillaClassName>`);
//! `common`/`cookie`/`ping` packets are stamped here with the play ids. Type names are the vanilla class names
//! (`ClientboundAddEntityPacket`); nested packet classes become `<Outer><Inner>Packet`
//! (`ClientboundMoveEntityPosPacket`, `ServerboundMovePlayerPosRotPacket`). Registry-holder fields of *data-driven*
//! registries (enchantments, biomes, dimension types, variants, ...) carry the raw registry id (`i32`), static registries use
//! the typed `mc_data` ids. Codec-backed NBT payloads whose Rust model lives in a later layer are raw [`mc_nbt::Tag`]s.
use mc_nbt::Tag;

use super::common::{common_packets, cookie_packets};
use crate::codec::Holder;
use crate::{Decode, Encode, Packet};

pub mod chat;
pub mod debug;
pub mod entity;
pub mod entity_data;
pub mod inventory;
pub mod level;
pub mod misc;
pub mod serverbound;
pub mod types;

pub use chat::*;
pub use debug::*;
pub use entity::*;
pub use entity_data::*;
pub use inventory::*;
pub use level::*;
pub use misc::*;
pub use serverbound::*;
pub use types::*;

cookie_packets!(play);
common_packets!(play);

/// `ClientboundBundleDelimiterPacket`: marks the start/end of a bundle (the pipeline, not the codec, groups packets).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "bundle_delimiter")]
pub struct ClientboundBundleDelimiterPacket;

/// `ClientboundPongResponsePacket` (play instance of the ping packet).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "pong_response")]
pub struct ClientboundPongResponsePacket {
    pub time: i64,
}

/// `ServerboundPingRequestPacket` (play instance of the ping packet).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = play, flow = serverbound, name = "ping_request")]
pub struct ServerboundPingRequestPacket {
    pub time: i64,
}

/// `ClientboundShowDialogPacket.STREAM_CODEC`: `Holder<Dialog>` — registry id + 1, or `0` + the dialog's NBT
/// (`ByteBufCodecs.holder` with the trusted NBT codec; the dialog stays a raw tag).
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = play, flow = clientbound, name = "show_dialog")]
pub struct ClientboundShowDialogPacket {
    pub dialog: Holder<Tag>,
}
