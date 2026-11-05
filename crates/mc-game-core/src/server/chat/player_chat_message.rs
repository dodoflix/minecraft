//! `net.minecraft.network.chat.PlayerChatMessage`.

use super::filter_mask::FilterMaskExt;
use super::message_signature::{self, MessageSignature};
use super::signature::SignatureValidator;
use super::signed_message::{NIL_UUID, SignedMessageBody, SignedMessageLink};
use mc_protocol::codec::Instant;
use mc_protocol::packets::play::FilterMask;
use mc_text::Component;
use uuid::Uuid;

/// `MESSAGE_EXPIRES_AFTER_SERVER` (5 minutes) in milliseconds.
pub const MESSAGE_EXPIRES_AFTER_SERVER_MS: i64 = 5 * 60 * 1000;
/// `MESSAGE_EXPIRES_AFTER_CLIENT` (5 + 2 minutes) in milliseconds.
pub const MESSAGE_EXPIRES_AFTER_CLIENT_MS: i64 = MESSAGE_EXPIRES_AFTER_SERVER_MS + 2 * 60 * 1000;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerChatMessage {
    pub link: SignedMessageLink,
    pub signature: Option<MessageSignature>,
    pub signed_body: SignedMessageBody,
    pub unsigned_content: Option<Component>,
    pub filter_mask: FilterMask,
}

impl PlayerChatMessage {
    /// `PlayerChatMessage.system(content)`.
    pub fn system(content: impl Into<String>) -> Self {
        Self::unsigned(NIL_UUID, content)
    }

    pub fn unsigned(sender: Uuid, content: impl Into<String>) -> Self {
        Self {
            link: SignedMessageLink::unsigned(sender),
            signature: None,
            signed_body: SignedMessageBody::unsigned(content),
            unsigned_content: None,
            filter_mask: FilterMask::PassThrough,
        }
    }

    pub fn with_unsigned_content(&self, content: Component) -> Self {
        let unsigned = (content != Component::literal(self.signed_content())).then_some(content);
        Self { unsigned_content: unsigned, ..self.clone() }
    }

    pub fn remove_unsigned_content(&self) -> Self {
        Self { unsigned_content: None, ..self.clone() }
    }

    pub fn filter_with_mask(&self, filter_mask: FilterMask) -> Self {
        if self.filter_mask == filter_mask { self.clone() } else { Self { filter_mask, ..self.clone() } }
    }

    /// `filter(boolean filtered)`: keeps the mask when filtered, else pass-through.
    pub fn filter(&self, filtered: bool) -> Self {
        self.filter_with_mask(if filtered { self.filter_mask.clone() } else { FilterMask::PassThrough })
    }

    pub fn remove_signature(&self) -> Self {
        Self {
            link: SignedMessageLink::unsigned(self.sender()),
            signature: None,
            signed_body: SignedMessageBody::unsigned(self.signed_content()),
            unsigned_content: self.unsigned_content.clone(),
            filter_mask: self.filter_mask.clone(),
        }
    }

    /// `PlayerChatMessage.updateSignature`: the bytes a message signature covers.
    pub fn update_signature(out: &mut Vec<u8>, link: &SignedMessageLink, body: &SignedMessageBody) {
        out.extend_from_slice(&1i32.to_be_bytes());
        link.update_signature(out);
        body.update_signature(out);
    }

    pub fn verify(&self, validator: &dyn SignatureValidator) -> bool {
        let Some(signature) = &self.signature else { return false };
        let mut payload = Vec::new();
        Self::update_signature(&mut payload, &self.link, &self.signed_body);
        validator.validate(&payload, &signature.0)
    }

    pub fn signed_content(&self) -> &str {
        &self.signed_body.content
    }

    pub fn decorated_content(&self) -> Component {
        self.unsigned_content.clone().unwrap_or_else(|| Component::literal(self.signed_content()))
    }

    pub fn time_stamp(&self) -> Instant {
        self.signed_body.time_stamp
    }

    pub fn salt(&self) -> i64 {
        self.signed_body.salt
    }

    pub fn has_expired_server(&self, now: Instant) -> bool {
        now.0 > self.time_stamp().0.saturating_add(MESSAGE_EXPIRES_AFTER_SERVER_MS)
    }

    pub fn has_expired_client(&self, now: Instant) -> bool {
        now.0 > self.time_stamp().0.saturating_add(MESSAGE_EXPIRES_AFTER_CLIENT_MS)
    }

    pub fn sender(&self) -> Uuid {
        self.link.sender
    }

    pub fn is_system(&self) -> bool {
        self.sender() == NIL_UUID
    }

    pub fn has_signature(&self) -> bool {
        self.signature.is_some()
    }

    pub fn has_signature_from(&self, profile_id: Uuid) -> bool {
        self.has_signature() && self.link.sender == profile_id
    }

    pub fn is_fully_filtered(&self) -> bool {
        self.filter_mask.is_fully_filtered()
    }

    /// `PlayerChatMessage.describeSigned` (the time stamp prints like `Instant.toString`).
    pub fn describe_signed(message: &PlayerChatMessage) -> String {
        let last_seen: String =
            message.signed_body.last_seen.entries.iter().map(|s| format!("     {}\n", message_signature::describe(Some(s)))).collect();
        format!(
            "'{}' @ {}\n - From: {}/{}, message #{}\n - Salt: {}\n - Signature: {}\n - Last Seen: [\n{} ]\n",
            message.signed_body.content,
            super::instant_to_string(message.signed_body.time_stamp),
            message.link.sender,
            message.link.session_id,
            message.link.index,
            message.signed_body.salt,
            message_signature::describe(message.signature.as_ref()),
            last_seen
        )
    }
}
