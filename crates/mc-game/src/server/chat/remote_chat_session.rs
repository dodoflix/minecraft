//! `RemoteChatSession` and `ProfilePublicKey` (`world.entity.player.ProfilePublicKey`).

use super::ThrowingComponent;
use super::instant_now;
use super::signature::{RsaSha256Validator, SignatureValidator};
use super::signed_message::{KeyBasedDecoder, SignedMessageChain};
use super::signed_message_validator::KeyBased;
use mc_protocol::codec::Instant;
use mc_protocol::packets::play::{ProfilePublicKeyData, RemoteChatSessionData};
use mc_text::Component;
use std::sync::Arc;
use uuid::Uuid;

/// `ProfilePublicKey.EXPIRY_GRACE_PERIOD` (8 hours) in milliseconds.
pub const EXPIRY_GRACE_PERIOD_MS: i64 = 8 * 60 * 60 * 1000;

pub fn expired_profile_public_key() -> Component {
    Component::translatable("multiplayer.disconnect.expired_public_key", vec![])
}

/// `ProfilePublicKey.Data` helpers.
pub trait ProfilePublicKeyDataExt {
    fn has_expired(&self) -> bool;
    fn has_expired_with_grace(&self, grace_period_ms: i64) -> bool;
    /// `signedPayload(profileId)`: the bytes the Mojang services key signed.
    fn signed_payload(&self, profile_id: Uuid) -> Vec<u8>;
    fn create_signature_validator(&self) -> Option<Arc<dyn SignatureValidator>>;
}

impl ProfilePublicKeyDataExt for ProfilePublicKeyData {
    fn has_expired(&self) -> bool {
        self.expires_at < instant_now()
    }

    fn has_expired_with_grace(&self, grace_period_ms: i64) -> bool {
        Instant(self.expires_at.0.saturating_add(grace_period_ms)) < instant_now()
    }

    fn signed_payload(&self, profile_id: Uuid) -> Vec<u8> {
        let key = self.key.der();
        let mut payload = Vec::with_capacity(24 + key.len());
        payload.extend_from_slice(profile_id.as_bytes());
        payload.extend_from_slice(&self.expires_at.0.to_be_bytes());
        payload.extend_from_slice(key);
        payload
    }

    fn create_signature_validator(&self) -> Option<Arc<dyn SignatureValidator>> {
        RsaSha256Validator::from_der(self.key.der()).map(|v| Arc::new(v) as Arc<dyn SignatureValidator>)
    }
}

/// `ProfilePublicKey.createValidated`.
pub fn create_validated(
    validator: &dyn SignatureValidator,
    profile_id: Uuid,
    data: &ProfilePublicKeyData,
) -> Result<ProfilePublicKeyData, ThrowingComponent> {
    if validator.validate(&data.signed_payload(profile_id), &data.key_signature) {
        Ok(data.clone())
    } else {
        Err(ThrowingComponent::new(Component::translatable("multiplayer.disconnect.invalid_public_key_signature", vec![])))
    }
}

#[derive(Clone)]
pub struct RemoteChatSession {
    pub session_id: Uuid,
    pub profile_public_key: ProfilePublicKeyData,
    validator: Arc<dyn SignatureValidator>,
}

impl RemoteChatSession {
    /// `RemoteChatSession.Data.validate(profile, serviceSignatureValidator)`; the signature validator of the key itself
    /// is built from the key (a key that is not valid RSA cannot decode, so `from_der` only fails on corrupt data).
    pub fn validate(
        data: &RemoteChatSessionData,
        profile_id: Uuid,
        service_signature_validator: &dyn SignatureValidator,
    ) -> Result<RemoteChatSession, ThrowingComponent> {
        let key = create_validated(service_signature_validator, profile_id, &data.profile_public_key)?;
        Self::new(data.session_id, key).ok_or_else(|| ThrowingComponent::new(Component::translatable("multiplayer.disconnect.invalid_public_key_signature", vec![])))
    }

    pub fn new(session_id: Uuid, profile_public_key: ProfilePublicKeyData) -> Option<RemoteChatSession> {
        let validator = profile_public_key.create_signature_validator()?;
        Some(RemoteChatSession { session_id, profile_public_key, validator })
    }

    pub fn create_message_validator(&self, grace_period_ms: i64) -> KeyBased {
        let data = self.profile_public_key.clone();
        KeyBased::new(self.validator.clone(), Box::new(move || data.has_expired_with_grace(grace_period_ms)))
    }

    pub fn create_message_decoder(&self, profile_id: Uuid) -> KeyBasedDecoder {
        SignedMessageChain::new(profile_id, self.session_id).decoder(self.validator.clone(), self.profile_public_key.expires_at)
    }

    pub fn as_data(&self) -> RemoteChatSessionData {
        RemoteChatSessionData { session_id: self.session_id, profile_public_key: self.profile_public_key.clone() }
    }

    pub fn has_expired(&self) -> bool {
        self.profile_public_key.has_expired()
    }
}
