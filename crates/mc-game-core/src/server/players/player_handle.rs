//! `PlayerHandle`: what the player list (and anything that must reach *every* connected player) holds of a
//! `ServerPlayer`. A player is taken out of its level while its packet or tick is handled (ARCHITECTURE: take-out
//! pattern), but broadcasts, the tab list and chat must still reach it, so the connection, the outgoing chat state
//! and the vanilla `ServerPlayer` state other players can observe (tab-list entry, position, dimension, chat
//! visibility) live here, shared between the entity and the list. The entity keeps the mirror up to date
//! (`ServerPlayer::sync_handle`).

use crate::server::chat::chat_type::Bound;
use crate::server::chat::outgoing_chat_message::{self, OutgoingChatMessage, OutgoingChatState, SendOutcome};
use crate::server::chat::{PlayerChatMessage, RemoteChatSession};
use crate::server::network::ConnectionSender;
use crate::server::players::NameAndId;
use crate::world::entity::EntityId;
use crate::world::level::DimensionKey;
use mc_core::phys::Vec3;
use mc_protocol::codec::{EnumSet, GameProfile};
use mc_protocol::packets::common::ChatVisibility;
use mc_protocol::packets::play::{
    ClientboundPlayerInfoUpdateAction, ClientboundPlayerInfoUpdateEntry, ClientboundPlayerInfoUpdatePacket, ClientboundSystemChatPacket,
    GameType as WireGameType,
};
use mc_text::{ChatFormatting, Component};
use mc_data::enums::GameType;
use parking_lot::Mutex;
use std::sync::Arc;
use uuid::Uuid;

/// The mutable part of the mirror (everything vanilla reads off a `ServerPlayer` from outside its level).
#[derive(Clone, Debug)]
pub struct PlayerSharedState {
    pub dimension: DimensionKey,
    pub position: Vec3,
    /// `ServerPlayer.gameMode()`.
    pub game_mode: GameType,
    /// `ServerGamePacketListenerImpl.latency()`.
    pub latency: i32,
    /// `ServerPlayer.getTabListDisplayName()`.
    pub tab_list_display_name: Option<Component>,
    /// `isModelPartShown(HAT)`.
    pub show_hat: bool,
    /// `ServerPlayer.getTabListOrder()`.
    pub tab_list_order: i32,
    /// `ServerPlayer.getChatSession()` (as sent in the tab list).
    pub chat_session: Option<mc_protocol::packets::play::RemoteChatSessionData>,
    /// `ClientInformation.chatVisibility`.
    pub chat_visibility: ChatVisibility,
    /// `ClientInformation.allowsListing` (server list player sample).
    pub allows_listing: bool,
    /// `ClientInformation.textFilteringEnabled`.
    pub text_filtering_enabled: bool,
}

pub struct PlayerHandle {
    pub id: EntityId,
    pub uuid: Uuid,
    pub profile: Arc<GameProfile>,
    /// `ServerPlayer.getIpAddress()` (`InetAddresses.toAddrString`, `<unknown>` for non-IP peers).
    pub ip_address: String,
    pub connection: ConnectionSender,
    pub chat_out: Arc<Mutex<OutgoingChatState>>,
    pub shared: Arc<Mutex<PlayerSharedState>>,
}

impl Clone for PlayerHandle {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            uuid: self.uuid,
            profile: self.profile.clone(),
            ip_address: self.ip_address.clone(),
            connection: self.connection.clone(),
            chat_out: self.chat_out.clone(),
            shared: self.shared.clone(),
        }
    }
}

impl PlayerHandle {
    pub fn new(id: EntityId, profile: GameProfile, connection: ConnectionSender, ip_address: String, dimension: DimensionKey) -> Self {
        Self {
            id,
            uuid: profile.id,
            profile: Arc::new(profile),
            ip_address,
            connection,
            chat_out: Arc::new(Mutex::new(OutgoingChatState::default())),
            shared: Arc::new(Mutex::new(PlayerSharedState {
                dimension,
                position: Vec3::ZERO,
                game_mode: GameType::Survival,
                latency: 0,
                tab_list_display_name: None,
                show_hat: true,
                tab_list_order: 0,
                chat_session: None,
                chat_visibility: ChatVisibility::Full,
                allows_listing: false,
                text_filtering_enabled: false,
            })),
        }
    }

    /// Identity (vanilla compares `ServerPlayer` references): two handles of the same connection share their mirror.
    pub fn same_player(&self, other: &PlayerHandle) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    /// `ServerPlayer.shouldFilterMessageTo(receiver)`.
    pub fn should_filter_message_to(&self, receiver: &PlayerHandle) -> bool {
        !self.same_player(receiver) && (self.shared.lock().text_filtering_enabled || receiver.shared.lock().text_filtering_enabled)
    }

    pub fn name(&self) -> &str {
        &self.profile.name
    }

    pub fn name_and_id(&self) -> NameAndId {
        NameAndId::new(self.uuid, self.profile.name.clone())
    }

    pub fn dimension(&self) -> DimensionKey {
        self.shared.lock().dimension.clone()
    }

    pub fn position(&self) -> Vec3 {
        self.shared.lock().position
    }

    pub fn game_mode(&self) -> GameType {
        self.shared.lock().game_mode
    }

    pub fn allows_listing(&self) -> bool {
        self.shared.lock().allows_listing
    }

    pub fn set_chat_session(&self, session: Option<&RemoteChatSession>) {
        self.shared.lock().chat_session = session.map(RemoteChatSession::as_data);
    }

    /// `ClientboundPlayerInfoUpdatePacket.Entry(ServerPlayer)`.
    pub fn info_entry(&self) -> ClientboundPlayerInfoUpdateEntry {
        let state = self.shared.lock();
        ClientboundPlayerInfoUpdateEntry {
            profile_id: self.uuid,
            profile: Some((*self.profile).clone()),
            listed: true,
            latency: state.latency,
            game_mode: WireGameType::by_id(match state.game_mode {
                GameType::Survival => 0,
                GameType::Creative => 1,
                GameType::Adventure => 2,
                GameType::Spectator => 3,
            }),
            display_name: state.tab_list_display_name.clone(),
            show_hat: state.show_hat,
            list_order: state.tab_list_order,
            chat_session: state.chat_session.clone(),
        }
    }

    /// `acceptsSystemMessages(overlay)`.
    fn accepts_system_messages(&self, overlay: bool) -> bool {
        self.shared.lock().chat_visibility != ChatVisibility::Hidden || overlay
    }

    /// `acceptsChatMessages()`.
    fn accepts_chat_messages(&self) -> bool {
        self.shared.lock().chat_visibility == ChatVisibility::Full
    }

    /// `ServerPlayer.sendSystemMessage(message, overlay)`.
    pub fn send_system_message(&self, message: &Component, overlay: bool) {
        if self.accepts_system_messages(overlay) {
            self.connection.send(&ClientboundSystemChatPacket { content: message.clone(), overlay });
        }
    }

    /// `ServerPlayer.sendChatMessage(message, filtered, chatType)`.
    pub fn send_chat_message(&self, message: &OutgoingChatMessage, filtered: bool, chat_type: &Bound) {
        if !self.accepts_chat_messages() {
            return;
        }
        match message {
            OutgoingChatMessage::Disguised(content) => outgoing_chat_message::send_disguised_chat_message(&self.connection, content, chat_type),
            OutgoingChatMessage::Player(message) => {
                let filtered_message = message.filter(filtered);
                if !filtered_message.is_fully_filtered() {
                    self.send_player_chat_message(&filtered_message, chat_type);
                }
            }
        }
    }

    /// `ServerGamePacketListenerImpl.sendPlayerChatMessage` (disconnects past 4096 pending chats).
    pub fn send_player_chat_message(&self, message: &PlayerChatMessage, chat_type: &Bound) {
        let outcome = self.chat_out.lock().send_player_chat_message(&self.connection, message, chat_type);
        if outcome == SendOutcome::TooManyPending {
            self.disconnect(&Component::translatable("multiplayer.disconnect.too_many_pending_chats", vec![]));
        }
    }

    /// `ServerGamePacketListenerImpl.disconnect(reason)` for a connection in the play state.
    pub fn disconnect(&self, reason: &Component) {
        // the first reason wins and becomes the `lost connection: <reason>` text
        self.connection.disconnect(reason.clone());
    }
}

/// `ClientboundPlayerInfoUpdatePacket.createPlayerInitializing(players)`.
pub fn create_player_initializing<'a>(players: impl IntoIterator<Item = &'a PlayerHandle>) -> ClientboundPlayerInfoUpdatePacket {
    ClientboundPlayerInfoUpdatePacket {
        actions: ClientboundPlayerInfoUpdateAction::initializing(),
        entries: players.into_iter().map(PlayerHandle::info_entry).collect(),
    }
}

/// `new ClientboundPlayerInfoUpdatePacket(actions, players)`.
pub fn create_info_update<'a>(
    actions: EnumSet<ClientboundPlayerInfoUpdateAction>,
    players: impl IntoIterator<Item = &'a PlayerHandle>,
) -> ClientboundPlayerInfoUpdatePacket {
    ClientboundPlayerInfoUpdatePacket { actions, entries: players.into_iter().map(PlayerHandle::info_entry).collect() }
}

/// `Component.translatable("multiplayer.player.joined", name).withStyle(YELLOW)` helper.
pub fn yellow(component: Component) -> Component {
    component.with_format(ChatFormatting::Yellow)
}

/// Test support: a handle whose connection is a channel the test can drain.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::server::network::Outbound;
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    pub struct Wire(pub UnboundedReceiver<Outbound>);

    impl Wire {
        /// Packet ids (and `None` for a close) received so far.
        pub fn ids(&mut self) -> Vec<Option<i32>> {
            let mut out = Vec::new();
            while let Ok(item) = self.0.try_recv() {
                out.push(match item {
                    Outbound::Payload(bytes) | Outbound::Write(bytes) => Some(mc_protocol::codec::read_varint(&mut &bytes[..]).unwrap()),
                    Outbound::Close | Outbound::Disconnect(_) => None,
                    // protocol reconfiguration messages are not packets
                    Outbound::Flush | Outbound::SetCompression(_) | Outbound::EnableEncryption(_) | Outbound::SetState(_) | Outbound::Raw(_) => continue,
                });
            }
            out
        }

        pub fn payloads(&mut self) -> Vec<Arc<[u8]>> {
            let mut out = Vec::new();
            while let Ok(Outbound::Payload(bytes)) = self.0.try_recv() {
                out.push(bytes);
            }
            out
        }
    }

    pub fn handle(id: i32, name: &str, dimension: &str) -> (PlayerHandle, Wire) {
        let (tx, rx) = unbounded_channel();
        let uuid = mc_core::uuid_util::create_offline_player_uuid(name);
        let profile = GameProfile { id: uuid, name: name.to_owned(), properties: vec![] };
        let handle = PlayerHandle::new(id, profile, ConnectionSender::new(tx), "127.0.0.1".into(), DimensionKey::try_parse(dimension).unwrap());
        (handle, Wire(rx))
    }
}
