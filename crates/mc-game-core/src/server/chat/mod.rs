//! `net.minecraft.network.chat` (signed chat model: messages, signature chain, last-seen window, chat types) and
//! `server.network.FilteredText`. The packet structs are `mc_protocol::packets::play`; this is the behavior.
//! Handling of the serverbound chat packets lives in the play packet listener; broadcasting in `server::players`.

pub mod chat_decorator;
pub mod chat_type;
pub mod filter_mask;
pub mod filtered_text;
pub mod last_seen_messages;
pub mod message_signature;
pub mod message_signature_cache;
pub mod outgoing_chat_message;
pub mod player_chat_message;
pub mod remote_chat_session;
pub mod signature;
pub mod signed_message;
pub mod signed_message_validator;

pub use chat_type::Bound as ChatTypeBound;
pub use filter_mask::FilterMaskExt;
pub use filtered_text::FilteredText;
pub use last_seen_messages::{
    LastSeenMessages, LastSeenMessagesTracker, LastSeenMessagesValidator,
};
pub use message_signature::MessageSignature;
pub use message_signature_cache::MessageSignatureCache;
pub use outgoing_chat_message::{OutgoingChatMessage, OutgoingChatState};
pub use player_chat_message::PlayerChatMessage;
pub use remote_chat_session::RemoteChatSession;
pub use signed_message::{SignedMessageBody, SignedMessageChain, SignedMessageLink};

use mc_protocol::codec::Instant;
use mc_text::Component;

/// `ThrowingComponent`: an error whose message is a chat component (kick / disable reason).
#[derive(Clone, Debug, PartialEq)]
pub struct ThrowingComponent {
    component: Component,
}

impl ThrowingComponent {
    pub fn new(component: Component) -> Self {
        Self { component }
    }

    pub fn component(&self) -> &Component {
        &self.component
    }
}

impl std::fmt::Display for ThrowingComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.component.get_string())
    }
}

impl std::error::Error for ThrowingComponent {}

/// `Instant.now()` (millisecond precision is all the wire carries).
pub fn instant_now() -> Instant {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    Instant(ms)
}

/// `Instant.toString()` (ISO-8601 UTC, fraction only when non-zero).
pub fn instant_to_string(instant: Instant) -> String {
    use chrono::{DateTime, SecondsFormat};
    DateTime::from_timestamp_millis(instant.0).map_or_else(
        || instant.0.to_string(),
        |t| t.to_rfc3339_opts(SecondsFormat::AutoSi, true),
    )
}
