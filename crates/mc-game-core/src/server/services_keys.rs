//! `Services.servicesKeySet()` (authlib `MinecraftServicesKeyInfo.get`, `RetryableFetch`): the Mojang Services
//! `playerCertificateKeys` (`SHA1withRSA`) that sign players' profile public keys. A background thread ("Minecraft
//! Services Discovery") reads the discovery document, then the key set behind `authentication/getPublicKeys`, at server
//! start; a failure is retried after 5 minutes doubling up to 5 * 2^6, a success refreshed after 24 hours.
//!
//! Vanilla's `ServicesKeySet.keys` waits for the first attempt to finish on whichever thread asks, which is the game
//! thread for the status flag, the login packet and `ChatSessionUpdate`. Here the key set is empty (unavailable) until the
//! first attempt is done and nothing waits.

use crate::server::chat::signature::SignatureValidator;
use base64::Engine;
use parking_lot::Mutex;
use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use serde_json::Value;
use sha1::Sha1;
use std::sync::Arc;
use std::time::Duration;

/// `MinecraftServicesEnvironment.PROD` discovery URL.
const PROD_DISCOVERY_URL: &str = "https://discovery.minecraftservices.com/minecraft/client";
/// Replaces the discovery URL, like the system property `minecraft.api.discovery.host` does for authlib.
const DISCOVERY_URL_VARIABLE: &str = "MINECRAFT_API_DISCOVERY_HOST";
/// `MinecraftClient.CONNECT_TIMEOUT_MS` / `READ_TIMEOUT_MS`.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const REFRESH_INTERVAL: Duration = Duration::from_secs(24 * 3600);
const RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// `RetryableFetch.MAX_BACKOFF_EXPONENT`.
const MAX_BACKOFF_EXPONENT: u32 = 6;

/// `EnvironmentParser.getEnvironmentFromProperties` for the discovery host.
pub fn discovery_url() -> String {
    std::env::var(DISCOVERY_URL_VARIABLE).unwrap_or_else(|_| PROD_DISCOVERY_URL.to_owned())
}

/// `SignatureValidator.from(keySet, PROFILE_KEY)`: valid when any key validates.
struct KeySet(Vec<VerifyingKey<Sha1>>);

impl SignatureValidator for KeySet {
    fn validate(&self, payload: &[u8], signature: &[u8]) -> bool {
        let Ok(sig) = Signature::try_from(signature) else {
            return false;
        };
        self.0.iter().any(|key| key.verify(payload, &sig).is_ok())
    }
}

#[derive(Default)]
struct State {
    /// The first fetch attempt has finished (successfully or not).
    first_attempt_done: bool,
    keys: Option<Arc<KeySet>>,
}

/// The handle every user of `Services.profileKeySignatureValidator` / `canValidateProfileKeys` shares.
#[derive(Clone, Default)]
pub struct ServicesKeys(Arc<Mutex<State>>);

impl std::fmt::Debug for ServicesKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServicesKeys")
    }
}

impl ServicesKeys {
    /// `ServicesKeySet.EMPTY`: no keys, no fetch (tests, tools).
    pub fn unavailable() -> Self {
        let keys = Self::default();
        keys.0.lock().first_attempt_done = true;
        keys
    }

    /// Starts the background fetch against `discovery_url`.
    pub fn start(discovery_url: String) -> Self {
        Self::start_with(discovery_url, REFRESH_INTERVAL, RETRY_INTERVAL)
    }

    fn start_with(discovery_url: String, refresh: Duration, retry: Duration) -> Self {
        let keys = Self::default();
        let state = keys.0.clone();
        let spawned = std::thread::Builder::new()
            .name("Minecraft Services Discovery".into())
            .spawn(move || fetch_loop(&state, &discovery_url, refresh, retry));
        if let Err(e) = spawned {
            log::error!("Failed to start the Minecraft Services key fetch: {e}");
            keys.0.lock().first_attempt_done = true;
        }
        keys
    }

    /// The first fetch attempt is still running: no key can be validated yet, and none is known to be missing.
    pub fn is_pending(&self) -> bool {
        !self.0.lock().first_attempt_done
    }

    /// `Services.canValidateProfileKeys`.
    pub fn can_validate_profile_keys(&self) -> bool {
        self.0.lock().keys.as_ref().is_some_and(|k| !k.0.is_empty())
    }

    /// `Services.profileKeySignatureValidator`: `None` without keys.
    pub fn profile_key_signature_validator(&self) -> Option<Arc<dyn SignatureValidator>> {
        let keys = self.0.lock().keys.clone()?;
        if keys.0.is_empty() {
            return None;
        }
        Some(keys)
    }
}

/// `RetryableFetch$1.run`: fetch, then reschedule (refresh after a value exists, backoff while there is none).
fn fetch_loop(state: &Mutex<State>, discovery_url: &str, refresh: Duration, retry: Duration) {
    let mut failures = 0u32;
    loop {
        if let Ok(keys) = fetch(discovery_url) {
            state.lock().keys = Some(Arc::new(keys));
            failures = 0;
        }
        let delay = {
            let mut state = state.lock();
            state.first_attempt_done = true;
            if state.keys.is_some() {
                refresh
            } else {
                let exponent = failures.min(MAX_BACKOFF_EXPONENT);
                failures += 1;
                retry * (1 << exponent)
            }
        };
        std::thread::sleep(delay);
    }
}

fn get_json(client: &reqwest::blocking::Client, url: &str) -> Result<Value, String> {
    client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| e.to_string())
}

/// The discovery document, then `authentication/getPublicKeys` (`MinecraftServicesKeyInfo.fetch`).
fn fetch(discovery_url: &str) -> Result<KeySet, String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(HTTP_TIMEOUT)
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let keys_url = get_json(&client, discovery_url)
        .ok()
        .and_then(|document| {
            document
                .pointer("/discovery/authentication/endpoints/getPublicKeys/uri")?
                .as_str()
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            log::warn!(
                "Services are unavailable, unable to fetch uri for AUTHENTICATION/getPublicKeys"
            );
            log::warn!("Failed to connect\nServices are unavailable");
            "Services are unavailable".to_owned()
        })?;
    let body = get_json(&client, &keys_url).map_err(|e| {
        log::error!("Failed to request Minecraft Services public key: {e}");
        e
    })?;
    parse_keys(&body).map_err(|e| {
        log::error!("Received malformed Minecraft Services public key data: {e}");
        e
    })
}

/// `KeySetResponse.playerCertificateKeys` through `MinecraftServicesKeyInfo.parse`: any malformed key fails the set.
fn parse_keys(body: &Value) -> Result<KeySet, String> {
    let mut keys = Vec::new();
    for entry in body
        .get("playerCertificateKeys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let der = entry
            .get("publicKey")
            .and_then(Value::as_str)
            .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
            .ok_or("Invalid Minecraft Services public key!")?;
        let key = RsaPublicKey::from_public_key_der(&der)
            .map_err(|_| "Invalid Minecraft Services public key!")?;
        keys.push(VerifyingKey::new(key));
    }
    Ok(KeySet(keys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs1v15::SigningKey;
    use rsa::pkcs8::EncodePublicKey;
    use rsa::signature::{SignatureEncoding, Signer};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Instant;

    /// The mock key service: `/discovery` points at `/keys`, which serves `public_key` once `keys_ok` is set (and
    /// an error before). While `hold_discovery` is set the discovery request waits for a message on the returned sender.
    fn serve(
        public_key: &[u8],
        keys_ok: Arc<AtomicBool>,
        hold_discovery: bool,
    ) -> (String, mpsc::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let (release, released) = mpsc::channel::<()>();
        let key = base64::engine::general_purpose::STANDARD.encode(public_key);
        let served = base.clone();
        std::thread::spawn(move || {
            let mut held = hold_discovery;
            for stream in listener.incoming() {
                let mut stream = stream.expect("accept");
                let mut buffer = [0u8; 2048];
                let read = stream.read(&mut buffer).expect("read");
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request.split_whitespace().nth(1).unwrap_or("/");
                let body = if path == "/discovery" {
                    if std::mem::take(&mut held) {
                        let _ = released.recv();
                    }
                    format!(
                        r#"{{"discovery":{{"authentication":{{"endpoints":{{"getPublicKeys":{{"uri":"{served}/keys"}}}}}}}}}}"#
                    )
                } else if keys_ok.load(Ordering::SeqCst) {
                    format!(
                        r#"{{"profilePropertyKeys":[],"playerCertificateKeys":[{{"publicKey":"{key}"}}]}}"#
                    )
                } else {
                    String::new()
                };
                let status = if body.is_empty() {
                    "503 Service Unavailable"
                } else {
                    "200 OK"
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (format!("{base}/discovery"), release)
    }

    fn wait_until(what: &str, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn key_pair() -> (SigningKey<Sha1>, Vec<u8>) {
        let private =
            rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("generate a key");
        let der = private
            .to_public_key()
            .to_public_key_der()
            .expect("encode the key")
            .as_bytes()
            .to_vec();
        (SigningKey::new(private), der)
    }

    #[test]
    fn a_held_fetch_leaves_the_keys_unavailable_without_waiting() {
        let (signer, der) = key_pair();
        let (url, release) = serve(&der, Arc::new(AtomicBool::new(true)), true);
        let keys =
            ServicesKeys::start_with(url, Duration::from_secs(3600), Duration::from_millis(50));

        let started = Instant::now();
        assert!(keys.is_pending());
        assert!(!keys.can_validate_profile_keys());
        assert!(keys.profile_key_signature_validator().is_none());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "a query waited for the fetch"
        );

        release.send(()).expect("release the discovery request");
        wait_until("the first attempt", || !keys.is_pending());
        assert!(keys.can_validate_profile_keys());
        let validator = keys.profile_key_signature_validator().expect("a validator");
        let signature = signer.sign(b"payload").to_vec();
        assert!(validator.validate(b"payload", &signature));
        assert!(!validator.validate(b"other payload", &signature));
    }

    #[test]
    fn a_failed_fetch_is_retried_until_the_keys_arrive() {
        let (_, der) = key_pair();
        let keys_ok = Arc::new(AtomicBool::new(false));
        let (url, _release) = serve(&der, keys_ok.clone(), false);
        let keys =
            ServicesKeys::start_with(url, Duration::from_secs(3600), Duration::from_millis(50));

        wait_until("the first attempt", || !keys.is_pending());
        assert!(!keys.can_validate_profile_keys());
        assert!(keys.profile_key_signature_validator().is_none());

        keys_ok.store(true, Ordering::SeqCst);
        wait_until("the retry", || keys.can_validate_profile_keys());
    }

    #[test]
    fn unreachable_services_resolve_to_no_keys() {
        let closed = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/discovery", closed.local_addr().expect("address"));
        drop(closed);
        let keys =
            ServicesKeys::start_with(url, Duration::from_secs(3600), Duration::from_secs(3600));
        wait_until("the first attempt", || !keys.is_pending());
        assert!(!keys.can_validate_profile_keys());
    }
}
