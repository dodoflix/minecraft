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
use mc_protocol::auth::PROD_DISCOVERY_URL;
use parking_lot::Mutex;
use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use serde_json::Value;
use sha1::Sha1;
use std::sync::Arc;
use std::time::Duration;

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

const CLIENT_EXCEPTION: &str = "com.mojang.authlib.exceptions.MinecraftClientException";
/// How many times vanilla fetches the key set at start: `Services` and the session service each ask the discovery service for
/// their own `ServicesKeySet`, and each one is a `RetryableFetch` of its own.
const KEY_SET_FETCHES: usize = 2;

/// The text Java's `IOException` carries for the failures a loopback or network request can end in
/// (`Socket.connect`, `HttpURLConnection`).
fn io_message(error: &reqwest::Error) -> String {
    if error.is_dns() {
        // `UnknownHostException` carries the host name
        return error
            .url()
            .and_then(|u| u.host_str())
            .unwrap_or_default()
            .to_owned();
    }
    if error.is_timeout() {
        return if error.is_connect() {
            "Connect timed out"
        } else {
            "Read timed out"
        }
        .into();
    }
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(e) = source {
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            return match io.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    "Connection refused (connect failed)".into()
                }
                std::io::ErrorKind::TimedOut if error.is_connect() => "Connect timed out".into(),
                std::io::ErrorKind::TimedOut => "Read timed out".into(),
                _ => io.to_string(),
            };
        }
        source = e.source();
    }
    error.to_string()
}

/// `Map.toString` of a Gson value: strings bare, numbers as `Double`.
fn java_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n
            .as_f64()
            .map_or_else(|| n.to_string(), |f| format!("{f:?}")),
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{k}={}", java_value(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(java_value).collect::<Vec<_>>().join(", ")
        ),
        other => other.to_string(),
    }
}

/// `ObjectMapper.readValue`: `Ok(None)` for an empty body (Gson answers null), the object for a JSON object, else the
/// exception Gson's parse failure becomes.
fn read_value(body: &str) -> Result<Option<Value>, String> {
    let failed = || format!("{CLIENT_EXCEPTION}: Failed to read value {body}");
    if body.trim().is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Null) => Ok(None),
        Ok(value @ Value::Object(_)) => Ok(Some(value)),
        _ => Err(failed()),
    }
}

/// `MinecraftClientHttpException.toString()` for an error answer (`MinecraftClient.buildHttpException` reads the body as an
/// `ErrorResponse` unless it is an HTML page, which it logs); a body that is not an object fails the way `readValue` does.
fn http_exception(url: &str, status: u16, content_type: &str, body: &str) -> String {
    let response = if content_type.starts_with("text/html") {
        log::error!("Got an error with a html body connecting to {url}: {body}");
        "null".to_owned()
    } else {
        match read_value(body) {
            Ok(Some(fields)) => {
                let field = |name: &str| fields.get(name).map_or("null".to_owned(), java_value);
                format!(
                    "ErrorResponse[path={}, error={}, errorMessage={}, details={}]",
                    field("path"),
                    field("error"),
                    field("errorMessage"),
                    field("details")
                )
            }
            Ok(None) => "null".to_owned(),
            Err(exception) => return exception,
        }
    };
    format!("MinecraftClientHttpException[type=HTTP_ERROR, status={status}, response={response}]")
}

/// `MinecraftClient.get`: the JSON object of the body (`None` for an empty one), or the first line of the exception
/// vanilla would throw.
fn get_json(client: &reqwest::blocking::Client, url: &str) -> Result<Option<Value>, String> {
    let response = client.get(url).send().map_err(|e| {
        format!(
            "{CLIENT_EXCEPTION}: Failed to read from {url} due to {}",
            io_message(&e)
        )
    })?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = response.text().map_err(|e| {
        format!(
            "{CLIENT_EXCEPTION}: Failed to read from {url} due to {}",
            io_message(&e)
        )
    })?;
    if status >= 400 {
        return Err(http_exception(url, status, &content_type, &body));
    }
    read_value(&body)
}

/// What `RetryableFetch` logs when the fetch throws.
fn log_fetch_failure(exception: &str) {
    log::warn!("Failed to connect\n{exception}");
}

/// One `getServicesKeySet` fetch: `getUrl(AUTHENTICATION, "getPublicKeys")` against the discovery document, then
/// `MinecraftServicesKeyInfo.fetch`.
fn fetch_key_set(client: &reqwest::blocking::Client, document: Option<&Value>) -> Option<KeySet> {
    let unavailable = format!("{CLIENT_EXCEPTION}: Services are unavailable");
    let Some(document) = document.filter(|d| d["environment"] != "offline") else {
        log::warn!(
            "Services are unavailable, unable to fetch uri for AUTHENTICATION/getPublicKeys"
        );
        log_fetch_failure(&unavailable);
        return None;
    };
    let Some(keys_url) = document
        .pointer("/discovery/authentication/endpoints/getPublicKeys/uri")
        .and_then(Value::as_str)
    else {
        log::warn!(
            "Services are available, but unable to find uri for AUTHENTICATION/getPublicKeys"
        );
        log_fetch_failure(&unavailable);
        return None;
    };
    let body = match get_json(client, keys_url) {
        Ok(Some(body)) => body,
        Ok(None) => return None,
        Err(exception) => {
            log::error!("Failed to request Minecraft Services public key\n{exception}");
            return None;
        }
    };
    match parse_keys(&body) {
        Ok(keys) => Some(keys),
        Err(_) => {
            log::error!(
                "Received malformed Minecraft Services public key data\njava.lang.IllegalArgumentException: Invalid Minecraft Services public key!"
            );
            None
        }
    }
}

/// The discovery document (`MinecraftServicesDiscoveryService.createDiscoverySupplier`), then the key set fetches in the
/// order vanilla runs them on its one discovery thread.
fn fetch(discovery_url: &str) -> Result<KeySet, String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(HTTP_TIMEOUT)
        .timeout(HTTP_TIMEOUT * 2) // Java allows the connect and the read 5 s each
        .build()
        .map_err(|e| e.to_string())?;
    let document = match get_json(&client, discovery_url) {
        Ok(document) => document,
        Err(exception) => {
            log_fetch_failure(&exception);
            None
        }
    };
    // the session service's fetch comes first and only logs; `Services.servicesKeySet` is the last one
    let mut keys = None;
    for _ in 0..KEY_SET_FETCHES {
        keys = fetch_key_set(&client, document.as_ref());
    }
    keys.ok_or_else(|| "no Minecraft Services public key".to_owned())
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
    fn error_answers_read_like_authlibs_exceptions() {
        let html = "text/html";
        let json = "application/json";
        assert_eq!(
            http_exception(
                "u",
                500,
                json,
                r#"{"errorMessage":"x","details":{"n":1,"a":"b"}}"#
            ),
            "MinecraftClientHttpException[type=HTTP_ERROR, status=500, response=ErrorResponse[path=null, error=null, errorMessage=x, details={n=1.0, a=b}]]"
        );
        assert_eq!(
            http_exception("u", 503, json, ""),
            "MinecraftClientHttpException[type=HTTP_ERROR, status=503, response=null]"
        );
        assert_eq!(
            http_exception("u", 502, json, "\"Bad Gateway\""),
            format!("{CLIENT_EXCEPTION}: Failed to read value \"Bad Gateway\"")
        );
        assert!(http_exception("u", 502, html, "<p>x</p>").ends_with("response=null]"));
    }

    #[test]
    fn bodies_read_like_gson_into_a_class() {
        assert_eq!(read_value("  "), Ok(None));
        assert_eq!(read_value("null"), Ok(None));
        assert_eq!(
            read_value(r#"{"a":1}"#),
            Ok(Some(serde_json::json!({"a": 1})))
        );
        assert!(read_value("[1]").is_err());
        assert!(read_value("{").is_err());
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
