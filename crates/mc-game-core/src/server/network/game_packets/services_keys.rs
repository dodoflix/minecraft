//! `Services.profileKeySignatureValidator()`: the Mojang Services `playerCertificateKeys` (authlib
//! `MinecraftServicesKeyInfo`, `SHA1withRSA`) that sign players' profile public keys. Fetched once in the background
//! the first time a player joins (vanilla fetches at startup); [`profile_key_signature_validator`] waits for the
//! fetch (bounded by the HTTP timeouts) and returns `None` when the keys are unavailable, in which case vanilla logs
//! `Ignoring chat session from {} due to missing Services public key`.

use crate::server::chat::signature::SignatureValidator;
use base64::Engine;
use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use serde_json::Value;
use sha1::Sha1;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

const KEYS_URL: &str = "https://api.minecraftservices.com/publickeys";

/// `SignatureValidator.from(keySet.keys(PROFILE_KEY), "SHA1withRSA")`: valid when any key validates.
struct ServicesKeys(Vec<VerifyingKey<Sha1>>);

impl SignatureValidator for ServicesKeys {
    fn validate(&self, payload: &[u8], signature: &[u8]) -> bool {
        let Ok(sig) = Signature::try_from(signature) else {
            return false;
        };
        self.0.iter().any(|key| key.verify(payload, &sig).is_ok())
    }
}

type Slot = (Mutex<Option<Option<Arc<dyn SignatureValidator>>>>, Condvar);

fn slot() -> &'static Slot {
    static SLOT: OnceLock<Slot> = OnceLock::new();
    SLOT.get_or_init(|| (Mutex::new(None), Condvar::new()))
}

fn fetch() -> Option<Arc<dyn SignatureValidator>> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let body: Value = match client
        .get(KEYS_URL)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
    {
        Ok(v) => v,
        Err(e) => {
            log::error!("Failed to request Minecraft Services public key: {e}");
            return None;
        }
    };
    let keys: Vec<VerifyingKey<Sha1>> = body
        .get("playerCertificateKeys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|k| k.get("publicKey")?.as_str())
        .filter_map(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
        .filter_map(|der| RsaPublicKey::from_public_key_der(&der).ok())
        .map(VerifyingKey::new)
        .collect();
    Some(Arc::new(ServicesKeys(keys)))
}

/// Starts the background fetch (idempotent).
pub fn start_fetch() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::Builder::new()
            .name("Services keys".into())
            .spawn(|| {
                let result = fetch();
                let (lock, cv) = slot();
                *lock.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                cv.notify_all();
            })
            .ok();
    });
}

/// The validator, waiting for the fetch if it is still running; `None` if the keys could not be fetched.
pub fn profile_key_signature_validator() -> Option<Arc<dyn SignatureValidator>> {
    start_fetch();
    let (lock, cv) = slot();
    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let (guard, _) = cv
        .wait_timeout_while(guard, Duration::from_secs(12), |r| r.is_none())
        .unwrap_or_else(|e| e.into_inner());
    guard.clone().flatten()
}
