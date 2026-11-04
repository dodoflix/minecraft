//! `OutgoingChatMessage` and the per-connection outgoing chat state (`ServerGamePacketListenerImpl`'s
//! `messageSignatureCache`, `lastSeenMessages` validator and `nextChatIndex`), shared between the player's packet
//! listener and its `PlayerHandle` so the player list can reach a player that is taken out of its level.

use super::chat_type::Bound;
use super::last_seen_messages::LastSeenMessagesValidator;
use super::message_signature_cache::MessageSignatureCache;
use super::player_chat_message::PlayerChatMessage;
use crate::server::network::ConnectionSender;
use mc_protocol::packets::play::{ClientboundDisguisedChatPacket, ClientboundPlayerChatPacket};
use mc_text::Component;

pub struct OutgoingChatState {
    pub message_signature_cache: MessageSignatureCache,
    pub last_seen_messages: LastSeenMessagesValidator,
    pub next_chat_index: i32,
}

impl Default for OutgoingChatState {
    fn default() -> Self {
        Self {
            message_signature_cache: MessageSignatureCache::create_default(),
            last_seen_messages: LastSeenMessagesValidator::new(20),
            next_chat_index: 0,
        }
    }
}

/// What `sendPlayerChatMessage` asks of its caller afterwards.
#[derive(Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    /// More than 4096 pending chats: vanilla disconnects with `multiplayer.disconnect.too_many_pending_chats`.
    TooManyPending,
}

impl OutgoingChatState {
    /// `ServerGamePacketListenerImpl.sendPlayerChatMessage`.
    pub fn send_player_chat_message(&mut self, connection: &ConnectionSender, message: &PlayerChatMessage, chat_type: &Bound) -> SendOutcome {
        let index = self.next_chat_index;
        self.next_chat_index = index.wrapping_add(1);
        connection.send(&ClientboundPlayerChatPacket {
            global_index: index,
            sender: message.link.sender,
            index: message.link.index,
            signature: message.signature.clone(),
            body: message.signed_body.pack(&self.message_signature_cache),
            unsigned_content: message.unsigned_content.clone(),
            filter_mask: message.filter_mask.clone(),
            chat_type: chat_type.to_packet(),
        });
        if let Some(signature) = &message.signature {
            self.message_signature_cache.push_body(&message.signed_body, Some(signature));
            self.last_seen_messages.add_pending(signature);
            if self.last_seen_messages.tracked_messages_count() > 4096 {
                return SendOutcome::TooManyPending;
            }
        }
        SendOutcome::Sent
    }
}

/// `sendDisguisedChatMessage`.
pub fn send_disguised_chat_message(connection: &ConnectionSender, content: &Component, chat_type: &Bound) {
    connection.send(&ClientboundDisguisedChatPacket { message: content.clone(), chat_type: chat_type.to_packet() });
}

/// `OutgoingChatMessage`.
#[derive(Clone, Debug)]
pub enum OutgoingChatMessage {
    Disguised(Component),
    Player(PlayerChatMessage),
}

impl OutgoingChatMessage {
    pub fn create(message: &PlayerChatMessage) -> Self {
        if message.is_system() { Self::Disguised(message.decorated_content()) } else { Self::Player(message.clone()) }
    }

    pub fn content(&self) -> Component {
        match self {
            Self::Disguised(content) => content.clone(),
            Self::Player(message) => message.decorated_content(),
        }
    }
}
