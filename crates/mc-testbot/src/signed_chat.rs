//! The client half of secure chat: the profile key pair from the services (`POST player.getCertificates`), the chat session
//! (`LocalChatSession`), the signature chain (`SignedMessageChain.Encoder`) and the last-seen tracker
//! (`LastSeenMessagesTracker`).
//!
//! Written from the vanilla client without sharing code with the server side, so that the server's validation is
//! checked against an independent implementation.
use base64::Engine;
use mc_protocol::auth::public_key_from_der;
use mc_protocol::codec::{BitSet, FixedBitSet, Instant, MessageSignature};
use mc_protocol::packets::play::{
    LastSeenMessagesUpdate, ProfilePublicKeyData, RemoteChatSessionData,
    ServerboundChatSessionUpdatePacket, ServerboundPlayPacket,
};
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

use crate::error::{BotError, Result};

/// `LastSeenMessages.LAST_SEEN_MESSAGES_MAX_LENGTH`
const LAST_SEEN: usize = 20;

/// The profile key pair of `KeyPairResponse`.
#[derive(Clone)]
pub struct ProfileKeyPair {
    private: RsaPrivateKey,
    /// X.509 `SubjectPublicKeyInfo`
    public_der: Vec<u8>,
    expires_at_millis: i64,
    /// `publicKeySignatureV2`
    key_signature: Vec<u8>,
}

impl ProfileKeyPair {
    /// `ProfileKeyPairManager`: the services entry `player.getCertificates` of the discovery document, called with
    /// the account's `access_token`.
    pub async fn fetch(discovery_url: &str, access_token: &str) -> Result<Self> {
        let http = reqwest::Client::new();
        let get =
            |what: &'static str, e: reqwest::Error| BotError::Protocol(format!("{what}: {e}"));
        let discovery: Value = serde_json::from_slice(
            &http
                .get(discovery_url)
                .send()
                .await
                .map_err(|e| get("discovery", e))?
                .bytes()
                .await
                .map_err(|e| get("discovery", e))?,
        )
        .map_err(|e| BotError::Protocol(format!("discovery document: {e}")))?;
        let url = discovery
            .pointer("/discovery/player/endpoints/getCertificates/uri")
            .and_then(Value::as_str)
            .ok_or_else(|| BotError::Protocol("no player.getCertificates endpoint".into()))?;
        let body = http
            .post(url)
            .bearer_auth(access_token)
            .header("Content-Type", "application/json")
            .body("")
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| get("getCertificates", e))?
            .bytes()
            .await
            .map_err(|e| get("getCertificates", e))?;
        let response: Value = serde_json::from_slice(&body)
            .map_err(|e| BotError::Protocol(format!("key pair response: {e}")))?;
        Self::from_response(&response)
    }

    fn from_response(response: &Value) -> Result<Self> {
        let bad = |what: &str| BotError::Protocol(format!("key pair response: {what}"));
        let text = |pointer: &str| {
            response
                .pointer(pointer)
                .and_then(Value::as_str)
                .ok_or_else(|| bad(pointer))
        };
        let b64 = base64::engine::general_purpose::STANDARD;
        let private = RsaPrivateKey::from_pkcs8_der(&pem_der(text("/keyPair/privateKey")?)?)
            .map_err(|e| bad(&e.to_string()))?;
        let expires_at = chrono::DateTime::parse_from_rfc3339(text("/expiresAt")?)
            .map_err(|e| bad(&e.to_string()))?;
        Ok(Self {
            private,
            public_der: pem_der(text("/keyPair/publicKey")?)?,
            expires_at_millis: expires_at.timestamp_millis(),
            key_signature: b64
                .decode(text("/publicKeySignatureV2")?)
                .map_err(|e| bad(&e.to_string()))?,
        })
    }
}

/// The DER bytes between the lines of a PEM block.
fn pem_der(pem: &str) -> Result<Vec<u8>> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|e| BotError::Protocol(format!("PEM: {e}")))
}

/// `SignedMessageBody` without the content's packed form: what a signature covers.
pub struct Body<'a> {
    pub content: &'a str,
    pub time_stamp_millis: i64,
    pub salt: i64,
    pub last_seen: &'a [MessageSignature],
}

/// `LocalChatSession` with its `SignedMessageChain` encoder.
#[derive(Debug)]
pub struct ChatSession {
    signer: SigningKey<Sha256>,
    profile_id: Uuid,
    session_id: Uuid,
    /// `SignedMessageChain.nextLink`'s index; `None` once the chain is exhausted.
    next_index: Option<i32>,
    data: RemoteChatSessionData,
}

impl ChatSession {
    pub fn new(profile_id: Uuid, keys: &ProfileKeyPair) -> Result<Self> {
        let session_id = Uuid::new_v4();
        let key = public_key_from_der(&keys.public_der)
            .map_err(|e| BotError::Protocol(format!("profile public key: {e}")))?;
        Ok(Self {
            signer: SigningKey::new(keys.private.clone()),
            profile_id,
            session_id,
            next_index: Some(0),
            data: RemoteChatSessionData {
                session_id,
                profile_public_key: ProfilePublicKeyData {
                    expires_at: Instant(keys.expires_at_millis),
                    key,
                    key_signature: keys.key_signature.clone(),
                },
            },
        })
    }

    /// `ServerboundChatSessionUpdatePacket(asRemote().asData())`
    pub fn update_packet(&self) -> ServerboundPlayPacket {
        ServerboundPlayPacket::ChatSessionUpdate(ServerboundChatSessionUpdatePacket {
            chat_session: self.data.clone(),
        })
    }

    /// `SignedMessageChain.Encoder.pack`: signs `body` with the next link of the chain and advances it.
    pub fn sign(&mut self, body: &Body) -> Option<MessageSignature> {
        let index = self.next_index?;
        self.next_index = index.checked_add(1);
        // `PlayerChatMessage.updateSignature`
        let mut payload = Vec::new();
        payload.extend_from_slice(&1i32.to_be_bytes());
        payload.extend_from_slice(self.profile_id.as_bytes());
        payload.extend_from_slice(self.session_id.as_bytes());
        payload.extend_from_slice(&index.to_be_bytes());
        payload.extend_from_slice(&body.salt.to_be_bytes());
        payload.extend_from_slice(&body.time_stamp_millis.div_euclid(1000).to_be_bytes());
        payload.extend_from_slice(&(body.content.len() as i32).to_be_bytes());
        payload.extend_from_slice(body.content.as_bytes());
        payload.extend_from_slice(&(body.last_seen.len() as i32).to_be_bytes());
        for signature in body.last_seen {
            payload.extend_from_slice(&signature.0);
        }
        let signature = self.signer.sign(&payload).to_vec();
        Some(MessageSignature(signature.try_into().ok()?))
    }
}

/// `LastSeenTrackedEntry`: a message and whether the server was already told that the client has seen it.
#[derive(Clone, Debug)]
struct Tracked {
    signature: MessageSignature,
    pending: bool,
}

/// `LastSeenMessagesTracker(20)`
#[derive(Debug)]
pub struct LastSeenTracker {
    messages: [Option<Tracked>; LAST_SEEN],
    tail: usize,
    offset: i32,
    last: Option<MessageSignature>,
}

impl Default for LastSeenTracker {
    fn default() -> Self {
        Self {
            messages: std::array::from_fn(|_| None),
            tail: 0,
            offset: 0,
            last: None,
        }
    }
}

impl LastSeenTracker {
    /// `addPending`: `false` for a repeat of the previous message.
    pub fn add_pending(&mut self, message: &MessageSignature) -> bool {
        if self.last.as_ref() == Some(message) {
            return false;
        }
        self.last = Some(message.clone());
        self.messages[self.tail] = Some(Tracked {
            signature: message.clone(),
            pending: true,
        });
        self.tail = (self.tail + 1) % LAST_SEEN;
        self.offset += 1;
        true
    }

    pub fn offset(&self) -> i32 {
        self.offset
    }

    /// `getAndClearOffset`
    pub fn take_offset(&mut self) -> i32 {
        std::mem::take(&mut self.offset)
    }

    /// `generateAndApplyUpdate`: the messages the packet acknowledges (what the signature covers) and the update.
    pub fn generate_and_apply_update(&mut self) -> (Vec<MessageSignature>, LastSeenMessagesUpdate) {
        let offset = self.take_offset();
        let mut acknowledged = BitSet::new();
        let mut seen = Vec::new();
        for i in 0..LAST_SEEN {
            if let Some(entry) = &mut self.messages[(self.tail + i) % LAST_SEEN] {
                acknowledged.set(i);
                seen.push(entry.signature.clone());
                entry.pending = false;
            }
        }
        let update = LastSeenMessagesUpdate {
            offset,
            acknowledged: FixedBitSet(acknowledged),
            checksum: checksum(&seen),
        };
        (seen, update)
    }
}

/// `LastSeenMessages.computeChecksum`
fn checksum(entries: &[MessageSignature]) -> i8 {
    let sum = entries.iter().fold(1i32, |sum, signature| {
        // `MessageSignature.checksum`: `Arrays.hashCode(bytes)`, the bytes signed
        let hash = signature.0.iter().fold(1i32, |h, &b| {
            h.wrapping_mul(31).wrapping_add(i32::from(b as i8))
        });
        sum.wrapping_mul(31).wrapping_add(hash)
    });
    match sum as i8 {
        0 => 1,
        byte => byte,
    }
}
