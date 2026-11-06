//! `configuration` state.
use mc_core::ident::Identifier;
use mc_nbt::Tag;

use super::common::{common_packets, cookie_packets};
use crate::{Decode, Encode, Packet, packet_enum};

cookie_packets!(configuration);
common_packets!(configuration);

/// `RegistrySynchronization.PackedRegistryEntry`: `data` is `None` when the client knows the entry from a known pack.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PackedRegistryEntry {
    pub id: Identifier,
    pub data: Option<Tag>,
}

/// `KnownPack`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

/// `ClientboundCodeOfConductPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = clientbound, name = "code_of_conduct")]
pub struct ClientboundCodeOfConductPacket {
    pub code_of_conduct: String,
}

/// `ClientboundFinishConfigurationPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = clientbound, name = "finish_configuration")]
pub struct ClientboundFinishConfigurationPacket;

/// `ClientboundRegistryDataPacket`
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = clientbound, name = "registry_data")]
pub struct ClientboundRegistryDataPacket {
    pub registry: Identifier,
    pub entries: Vec<PackedRegistryEntry>,
}

/// `ClientboundResetChatPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = clientbound, name = "reset_chat")]
pub struct ClientboundResetChatPacket;

/// `ClientboundSelectKnownPacks`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = clientbound, name = "select_known_packs")]
pub struct ClientboundSelectKnownPacks {
    pub known_packs: Vec<KnownPack>,
}

/// `ClientboundShowDialogPacket.CONTEXT_FREE_STREAM_CODEC`: the dialog's NBT, no holder id.
#[derive(Debug, Clone, PartialEq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = clientbound, name = "show_dialog")]
pub struct ClientboundShowDialogPacket {
    pub dialog: crate::dialog::Dialog,
}

/// `ClientboundUpdateEnabledFeaturesPacket` (`Set<Identifier>`: `HashSet`, repeated features collapse on decode).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = clientbound, name = "update_enabled_features")]
pub struct ClientboundUpdateEnabledFeaturesPacket {
    #[mc(set)]
    pub features: Vec<Identifier>,
}

/// `ServerboundAcceptCodeOfConductPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = serverbound, name = "accept_code_of_conduct")]
pub struct ServerboundAcceptCodeOfConductPacket;

/// `ServerboundFinishConfigurationPacket`
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = serverbound, name = "finish_configuration")]
pub struct ServerboundFinishConfigurationPacket;

/// `ServerboundSelectKnownPacks` (at most 64 packs)
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Packet)]
#[packet(state = configuration, flow = serverbound, name = "select_known_packs")]
pub struct ServerboundSelectKnownPacks {
    #[mc(max = 64)]
    pub known_packs: Vec<KnownPack>,
}

packet_enum! {
    /// Clientbound configuration packets, in packet-id order of `packets.json`.
    pub enum ClientboundConfigurationPacket {
        CookieRequest(ClientboundCookieRequestPacket),
        CustomPayload(ClientboundCustomPayloadPacket),
        Disconnect(ClientboundDisconnectPacket),
        FinishConfiguration(ClientboundFinishConfigurationPacket),
        KeepAlive(ClientboundKeepAlivePacket),
        Ping(ClientboundPingPacket),
        ResetChat(ClientboundResetChatPacket),
        RegistryData(ClientboundRegistryDataPacket),
        ResourcePackPop(ClientboundResourcePackPopPacket),
        ResourcePackPush(ClientboundResourcePackPushPacket),
        PostEffects(ClientboundPostEffectsPacket),
        StoreCookie(ClientboundStoreCookiePacket),
        Transfer(ClientboundTransferPacket),
        UpdateEnabledFeatures(ClientboundUpdateEnabledFeaturesPacket),
        UpdateTags(ClientboundUpdateTagsPacket),
        SelectKnownPacks(ClientboundSelectKnownPacks),
        CustomReportDetails(ClientboundCustomReportDetailsPacket),
        ServerLinks(ClientboundServerLinksPacket),
        ClearDialog(ClientboundClearDialogPacket),
        ShowDialog(ClientboundShowDialogPacket),
        CodeOfConduct(ClientboundCodeOfConductPacket),
    }
}

packet_enum! {
    /// Serverbound configuration packets.
    pub enum ServerboundConfigurationPacket {
        ClientInformation(ServerboundClientInformationPacket),
        CookieResponse(ServerboundCookieResponsePacket),
        CustomPayload(ServerboundCustomPayloadPacket),
        FinishConfiguration(ServerboundFinishConfigurationPacket),
        KeepAlive(ServerboundKeepAlivePacket),
        Pong(ServerboundPongPacket),
        ResourcePack(ServerboundResourcePackPacket),
        SelectKnownPacks(ServerboundSelectKnownPacks),
        CustomClickAction(ServerboundCustomClickActionPacket),
        AcceptCodeOfConduct(ServerboundAcceptCodeOfConductPacket),
    }
}
