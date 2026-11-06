//! `SignedMessageLink`, `SignedMessageBody`, `SignedMessageChain` (+ `Decoder`/`Encoder`, `DecodeException`).

use super::last_seen_messages::{self, LastSeenMessages};
use super::message_signature::MessageSignature;
use super::message_signature_cache::{IdOutOfRange, MessageSignatureCache};
use super::player_chat_message::PlayerChatMessage;
use super::signature::SignatureValidator;
use super::{ThrowingComponent, instant_now};
use mc_protocol::codec::Instant;
use mc_protocol::packets::play::SignedMessageBodyPacked;
use mc_text::Component;
use std::sync::Arc;
use uuid::Uuid;

/// `Util.NIL_UUID`.
pub const NIL_UUID: Uuid = Uuid::nil();

/// `SignedMessageLink(index, sender, sessionId)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedMessageLink {
    pub index: i32,
    pub sender: Uuid,
    pub session_id: Uuid,
}

impl SignedMessageLink {
    pub fn unsigned(sender: Uuid) -> Self {
        Self::root(sender, NIL_UUID)
    }

    pub fn root(sender: Uuid, session_id: Uuid) -> Self {
        Self {
            index: 0,
            sender,
            session_id,
        }
    }

    /// `updateSignature`: sender, session id (16 bytes each, most significant first), index (big endian int).
    pub fn update_signature(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.sender.as_bytes());
        out.extend_from_slice(self.session_id.as_bytes());
        out.extend_from_slice(&self.index.to_be_bytes());
    }

    pub fn is_descendant_of(&self, link: &SignedMessageLink) -> bool {
        self.index > link.index && self.sender == link.sender && self.session_id == link.session_id
    }

    pub fn advance(&self) -> Option<SignedMessageLink> {
        (self.index != i32::MAX).then(|| Self {
            index: self.index + 1,
            ..self.clone()
        })
    }
}

/// `SignedMessageBody(content, timeStamp, salt, lastSeen)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedMessageBody {
    pub content: String,
    pub time_stamp: Instant,
    pub salt: i64,
    pub last_seen: LastSeenMessages,
}

impl SignedMessageBody {
    pub fn unsigned(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            time_stamp: instant_now(),
            salt: 0,
            last_seen: LastSeenMessages::empty(),
        }
    }

    pub fn update_signature(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.salt.to_be_bytes());
        out.extend_from_slice(&self.time_stamp.0.div_euclid(1000).to_be_bytes());
        out.extend_from_slice(&(self.content.len() as i32).to_be_bytes());
        out.extend_from_slice(self.content.as_bytes());
        self.last_seen.update_signature(out);
    }

    pub fn pack(&self, cache: &MessageSignatureCache) -> SignedMessageBodyPacked {
        SignedMessageBodyPacked {
            content: self.content.clone(),
            time_stamp: self.time_stamp,
            salt: self.salt,
            last_seen: self.last_seen.pack(cache),
        }
    }
}

/// `SignedMessageBody.Packed.unpack`: `Ok(None)` = an entry is not in the cache (vanilla `Optional.empty()`).
pub fn unpack_body(
    packed: &SignedMessageBodyPacked,
    cache: &MessageSignatureCache,
) -> Result<Option<SignedMessageBody>, IdOutOfRange> {
    Ok(
        last_seen_messages::unpack(&packed.last_seen, cache)?.map(|last_seen| SignedMessageBody {
            content: packed.content.clone(),
            time_stamp: packed.time_stamp,
            salt: packed.salt,
            last_seen,
        }),
    )
}

/// `SignedMessageChain.DecodeException`: the component is the kick/disable reason.
pub type DecodeException = ThrowingComponent;

pub mod decode_exception {
    use super::*;
    pub fn missing_profile_key() -> DecodeException {
        ThrowingComponent::new(Component::translatable(
            "chat.disabled.missingProfileKey",
            vec![],
        ))
    }
    pub fn chain_broken() -> DecodeException {
        ThrowingComponent::new(Component::translatable(
            "chat.disabled.chain_broken",
            vec![],
        ))
    }
    pub fn expired_profile_key() -> DecodeException {
        ThrowingComponent::new(Component::translatable(
            "chat.disabled.expiredProfileKey",
            vec![],
        ))
    }
    pub fn invalid_signature() -> DecodeException {
        ThrowingComponent::new(Component::translatable(
            "chat.disabled.invalid_signature",
            vec![],
        ))
    }
    pub fn out_of_order_chat() -> DecodeException {
        ThrowingComponent::new(Component::translatable(
            "chat.disabled.out_of_order_chat",
            vec![],
        ))
    }
}

/// `SignedMessageChain.Decoder`.
pub trait Decoder: Send {
    fn unpack(
        &mut self,
        signature: Option<&MessageSignature>,
        body: SignedMessageBody,
    ) -> Result<PlayerChatMessage, DecodeException>;
    fn set_chain_broken(&mut self) {}
}

/// `SignedMessageChain.Decoder.unsigned(profileId, enforcesSecureChat)`.
pub struct UnsignedDecoder {
    profile_id: Uuid,
    enforces_secure_chat: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl UnsignedDecoder {
    pub fn new(
        profile_id: Uuid,
        enforces_secure_chat: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            profile_id,
            enforces_secure_chat,
        }
    }
}

impl Decoder for UnsignedDecoder {
    fn unpack(
        &mut self,
        _signature: Option<&MessageSignature>,
        body: SignedMessageBody,
    ) -> Result<PlayerChatMessage, DecodeException> {
        if (self.enforces_secure_chat)() {
            Err(decode_exception::missing_profile_key())
        } else {
            Ok(PlayerChatMessage::unsigned(self.profile_id, body.content))
        }
    }
}

/// `SignedMessageChain`: the chain state (next link, last timestamp); `decoder` consumes it, as vanilla's decoder
/// is the only thing that ever touches it again.
pub struct SignedMessageChain {
    next_link: Option<SignedMessageLink>,
    last_time_stamp: Instant,
}

impl SignedMessageChain {
    pub fn new(profile_id: Uuid, session_id: Uuid) -> Self {
        Self {
            next_link: Some(SignedMessageLink::root(profile_id, session_id)),
            last_time_stamp: Instant(0),
        }
    }

    /// `encoder(signer)`: signs bodies; `None` once the chain index is exhausted.
    pub fn encode(
        &mut self,
        body: &SignedMessageBody,
        sign: impl FnOnce(&[u8]) -> Vec<u8>,
    ) -> Option<MessageSignature> {
        let link = self.next_link.take()?;
        self.next_link = link.advance();
        let mut payload = Vec::new();
        PlayerChatMessage::update_signature(&mut payload, &link, body);
        sign(&payload).try_into().ok().map(MessageSignature)
    }

    /// `decoder(profilePublicKey)`. `expires_at` is `profilePublicKey.data().expiresAt()`.
    pub fn decoder(
        self,
        validator: Arc<dyn SignatureValidator>,
        expires_at: Instant,
    ) -> KeyBasedDecoder {
        KeyBasedDecoder {
            chain: self,
            validator,
            expires_at,
        }
    }
}

pub struct KeyBasedDecoder {
    chain: SignedMessageChain,
    validator: Arc<dyn SignatureValidator>,
    expires_at: Instant,
}

impl Decoder for KeyBasedDecoder {
    fn unpack(
        &mut self,
        signature: Option<&MessageSignature>,
        body: SignedMessageBody,
    ) -> Result<PlayerChatMessage, DecodeException> {
        let Some(signature) = signature else {
            return Err(decode_exception::missing_profile_key());
        };
        if self.expires_at < instant_now() {
            return Err(decode_exception::expired_profile_key());
        }
        let Some(link) = self.chain.next_link.clone() else {
            return Err(decode_exception::chain_broken());
        };
        if body.time_stamp < self.chain.last_time_stamp {
            self.set_chain_broken();
            return Err(decode_exception::out_of_order_chat());
        }
        self.chain.last_time_stamp = body.time_stamp;
        let unpacked = PlayerChatMessage {
            link: link.clone(),
            signature: Some(signature.clone()),
            signed_body: body,
            unsigned_content: None,
            filter_mask: Default::default(),
        };
        if !unpacked.verify(self.validator.as_ref()) {
            self.set_chain_broken();
            return Err(decode_exception::invalid_signature());
        }
        if unpacked.has_expired_server(instant_now()) {
            log::warn!(
                "Received expired chat: '{}'. Is the client/server system time unsynchronized?",
                unpacked.signed_body.content
            );
        }
        self.chain.next_link = link.advance();
        Ok(unpacked)
    }

    fn set_chain_broken(&mut self) {
        self.chain.next_link = None;
    }
}
