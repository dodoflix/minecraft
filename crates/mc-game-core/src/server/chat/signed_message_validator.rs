//! `SignedMessageValidator` (+ `ACCEPT_UNSIGNED`, `REJECT_ALL`, `KeyBased`): validates *received* signed messages
//! (used by clients; the server-side equivalent is `SignedMessageChain.Decoder`).

use super::player_chat_message::PlayerChatMessage;
use super::signature::SignatureValidator;
use std::sync::Arc;

pub trait SignedMessageValidator: Send {
    fn update_and_validate(&mut self, message: PlayerChatMessage) -> Option<PlayerChatMessage>;
}

/// `SignedMessageValidator.ACCEPT_UNSIGNED`.
pub struct AcceptUnsigned;

impl SignedMessageValidator for AcceptUnsigned {
    fn update_and_validate(&mut self, message: PlayerChatMessage) -> Option<PlayerChatMessage> {
        Some(message.remove_signature())
    }
}

/// `SignedMessageValidator.REJECT_ALL`.
pub struct RejectAll;

impl SignedMessageValidator for RejectAll {
    fn update_and_validate(&mut self, message: PlayerChatMessage) -> Option<PlayerChatMessage> {
        log::error!(
            "Received chat message from {}, but they have no chat session initialized and secure chat is enforced",
            message.sender()
        );
        None
    }
}

pub struct KeyBased {
    validator: Arc<dyn SignatureValidator>,
    expired: Box<dyn Fn() -> bool + Send>,
    last_message: Option<PlayerChatMessage>,
    is_chain_valid: bool,
}

impl KeyBased {
    pub fn new(
        validator: Arc<dyn SignatureValidator>,
        expired: Box<dyn Fn() -> bool + Send>,
    ) -> Self {
        Self {
            validator,
            expired,
            last_message: None,
            is_chain_valid: true,
        }
    }

    fn validate_chain(&self, message: &PlayerChatMessage) -> bool {
        if self.last_message.as_ref() == Some(message) {
            return true;
        }
        if let Some(last) = &self.last_message
            && !message.link.is_descendant_of(&last.link)
        {
            log::error!(
                "Received out-of-order chat message from {}: expected index > {} for session {}, but was {} for session {}",
                message.sender(),
                last.link.index,
                last.link.session_id,
                message.link.index,
                message.link.session_id
            );
            return false;
        }
        true
    }

    fn validate(&self, message: &PlayerChatMessage) -> bool {
        if (self.expired)() {
            log::error!(
                "Received message with expired profile public key from {} with session {}",
                message.sender(),
                message.link.session_id
            );
            false
        } else if !message.verify(self.validator.as_ref()) {
            log::error!(
                "Received message with invalid signature (is the session wrong, or signature cache out of sync?): {}",
                PlayerChatMessage::describe_signed(message)
            );
            false
        } else {
            self.validate_chain(message)
        }
    }
}

impl SignedMessageValidator for KeyBased {
    fn update_and_validate(&mut self, message: PlayerChatMessage) -> Option<PlayerChatMessage> {
        self.is_chain_valid = self.is_chain_valid && self.validate(&message);
        if !self.is_chain_valid {
            return None;
        }
        self.last_message = Some(message.clone());
        Some(message)
    }
}
