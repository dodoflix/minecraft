//! `Services.servicesKeySet()` (authlib `MinecraftServicesKeyInfo.get`, `RetryableFetch`): the Mojang Services
//! `playerCertificateKeys` (`SHA1withRSA`) that sign players' profile public keys. A background thread ("Minecraft
//! Services Discovery") reads the discovery document, then twice the key set behind `authentication/getPublicKeys`
//! (the session service's own fetch and `Services.servicesKeySet`), at server start. Each of the three is its own
//! `RetryableFetch`: a failure is retried after 1 minute (the document) or 5 minutes (a key set) doubling up to 2^6
//! times that, a success refreshed after 24 hours.
//!
//! Vanilla's `ServicesKeySet.keys` waits for the first attempt to finish on whichever thread asks, which is the game
//! thread for the status flag, the login packet and `ChatSessionUpdate`. Here the key set is empty (unavailable) until the
//! first attempt is done and nothing waits.

use crate::server::chat::signature::SignatureValidator;
use base64::Engine;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use mc_protocol::auth::PROD_DISCOVERY_URL;
use parking_lot::Mutex;
use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use serde_json::Value;
use sha1::Sha1;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Replaces the discovery URL, like the system property `minecraft.api.discovery.host` does for authlib.
const DISCOVERY_URL_VARIABLE: &str = "MINECRAFT_API_DISCOVERY_HOST";
/// `MinecraftClient.CONNECT_TIMEOUT_MS` / `READ_TIMEOUT_MS`.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const REFRESH_INTERVAL: Duration = Duration::from_secs(24 * 3600);
const KEY_RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);
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
        Self::start_with(discovery_url, REFRESH_INTERVAL, KEY_RETRY_INTERVAL)
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

/// What the discovery thread runs, in the order vanilla queues them at start.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Task {
    Discovery,
    /// The `n`th key set fetch; the last one is the one `Services.servicesKeySet` holds.
    KeySet(usize),
}

/// `RetryableFetch$1.reschedule`: a refresh once there is a value, else the backoff doubling per failure.
fn next_delay(has_value: bool, failures: &mut u32, refresh: Duration, retry: Duration) -> Duration {
    if has_value {
        return refresh;
    }
    let exponent = (*failures).min(MAX_BACKOFF_EXPONENT);
    *failures += 1;
    retry * (1 << exponent)
}

/// The single thread of vanilla's `DISCOVERY_EXECUTOR`: the discovery document and each key set are a `RetryableFetch`
/// of their own on it, run by their due time. A key set fetch reads the document as it stands then (the offline
/// stand-in until a fetch of it succeeded).
fn fetch_loop(state: &Mutex<State>, discovery_url: &str, refresh: Duration, retry: Duration) {
    let Ok(client) = reqwest::blocking::Client::builder()
        .connect_timeout(HTTP_TIMEOUT)
        .timeout(HTTP_TIMEOUT * 2) // Java allows the connect and the read 5 s each
        .build()
    else {
        state.lock().first_attempt_done = true;
        return;
    };
    let mut document: Option<Value> = None;
    let mut failures = [0u32; 1 + KEY_SET_FETCHES];
    let mut has_keys = [false; KEY_SET_FETCHES];
    let mut queue = BinaryHeap::new();
    let now = Instant::now();
    queue.push(Reverse((now, Task::Discovery)));
    for n in 0..KEY_SET_FETCHES {
        queue.push(Reverse((now, Task::KeySet(n))));
    }
    while let Some(Reverse((due, task))) = queue.pop() {
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
        let delay = match task {
            Task::Discovery => {
                match get_json(&client, discovery_url, |document| Ok(document.clone())) {
                    Ok(Some(fetched)) => {
                        document = Some(fetched);
                        failures[0] = 0;
                    }
                    Ok(None) => {}
                    Err(exception) => log_fetch_failure(&exception),
                }
                // the document's own fetch retries after 1 minute where a key set's takes 5
                next_delay(document.is_some(), &mut failures[0], refresh, retry / 5)
            }
            Task::KeySet(n) => {
                if let Some(keys) = fetch_key_set(&client, document.as_ref()) {
                    has_keys[n] = true;
                    failures[1 + n] = 0;
                    if n == KEY_SET_FETCHES - 1 {
                        state.lock().keys = Some(Arc::new(keys));
                    }
                }
                if n == KEY_SET_FETCHES - 1 {
                    state.lock().first_attempt_done = true;
                }
                next_delay(has_keys[n], &mut failures[1 + n], refresh, retry)
            }
        };
        queue.push(Reverse((Instant::now() + delay, task)));
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

/// `Double.toString`: the shortest digits that round-trip, plain in `[1e-3, 1e7)`, else `d.dddE±n`.
fn java_double(f: f64) -> String {
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0" } else { "0.0" }.to_owned();
    }
    let sign = if f < 0.0 { "-" } else { "" };
    let mut scientific = format!("{:e}", f.abs());
    if !scientific.contains('.') {
        // a single shortest digit: the JDK takes the closest two-digit decimal, which only differs for subnormals
        scientific = format!("{:.1e}", f.abs());
    }
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits = mantissa.replace('.', "");
    if !(-3..7).contains(&exponent) {
        let fraction = digits.get(1..).filter(|d| !d.is_empty()).unwrap_or("0");
        return format!("{sign}{}.{fraction}E{exponent}", &digits[..1]);
    }
    if exponent < 0 {
        let zeros = "0".repeat((-exponent - 1) as usize);
        return format!("{sign}0.{zeros}{}", digits.trim_end_matches('0'));
    }
    let integer = exponent as usize + 1;
    let padded = format!("{digits:0<integer$}");
    let fraction = padded
        .get(integer..)
        .filter(|d| !d.is_empty())
        .unwrap_or("0");
    format!("{sign}{}.{fraction}", &padded[..integer])
}

/// `Map.toString` of a Gson value: strings bare, numbers as `Double`.
fn java_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), java_double),
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

/// Why Gson cannot fill a class from a JSON object: a field of a type it cannot take (authlib wraps that in a
/// `MinecraftClientException`), or a null map key, whose `NullPointerException` nothing wraps.
#[derive(Debug, PartialEq)]
enum Unread {
    Malformed,
    NullKey,
}

const NULL_MAP_KEY: &str = "java.lang.NullPointerException: key == null";

/// `ObjectMapper.readValue`: `Ok(None)` for an empty body (Gson answers null), the JSON object read as the class `read`
/// describes, else the exception Gson's failure becomes.
fn read_value<T>(
    body: &str,
    read: impl Fn(&Value) -> Result<T, Unread>,
) -> Result<Option<T>, String> {
    let failed = || format!("{CLIENT_EXCEPTION}: Failed to read value {body}");
    if body.trim().is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Null) => Ok(None),
        Ok(value @ Value::Object(_)) => read(&value).map(Some).map_err(|e| match e {
            Unread::Malformed => failed(),
            Unread::NullKey => NULL_MAP_KEY.to_owned(),
        }),
        _ => Err(failed()),
    }
}

/// Gson reads a JSON array into a `Map<String, Object>` as `[key, value]` pairs (`MapTypeAdapterFactory`): exactly two
/// elements each and a scalar key; a repeated key fails, whatever value the first one had. A null key is stored before
/// the closing bracket is read, so `LinkedTreeMap.put` throws whatever follows it (a missing value fails first).
fn pairs_as_map(pairs: &[Value]) -> Result<String, Unread> {
    let mut entries: Vec<(String, &Value)> = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let items = pair.as_array().ok_or(Unread::Malformed)?;
        let (Some(key), Some(value)) = (items.first(), items.get(1)) else {
            return Err(Unread::Malformed);
        };
        let key = match key {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => return Err(Unread::NullKey),
            _ => return Err(Unread::Malformed),
        };
        if items.len() > 2 || entries.iter().any(|(k, _)| *k == key) {
            return Err(Unread::Malformed);
        }
        entries.push((key, value));
    }
    let items: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("{k}={}", java_value(v)))
        .collect();
    Ok(format!("{{{}}}", items.join(", ")))
}

/// `ErrorResponse.toString()` as Gson fills the record: its `String` fields take strings, booleans and numbers, `details`
/// a `Map<String, Object>`.
// shortcut: a number is printed as serde_json normalises it (`1.50` -> `1.5`, `100000000000000000000` -> `1e20`), Gson keeps the text as
// written; the same for a numeric `publicKey` in `key_lists`. Mojang sends neither; upgrade with serde_json's
// `arbitrary_precision` if it ever matters (a workspace-wide feature).
fn error_response(body: &Value) -> Result<String, Unread> {
    let text = |name: &str| match body.get(name) {
        None | Some(Value::Null) => Ok("null".to_owned()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Bool(b)) => Ok(b.to_string()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(_) => Err(Unread::Malformed),
    };
    let details = match body.get("details") {
        None | Some(Value::Null) => "null".to_owned(),
        Some(map @ Value::Object(_)) => java_value(map),
        Some(Value::Array(pairs)) => pairs_as_map(pairs)?,
        Some(_) => return Err(Unread::Malformed),
    };
    Ok(format!(
        "ErrorResponse[path={}, error={}, errorMessage={}, details={details}]",
        text("path")?,
        text("error")?,
        text("errorMessage")?
    ))
}

/// `MinecraftClientHttpException.toString()` for an error answer (`MinecraftClient.buildHttpException` reads the body as an
/// `ErrorResponse` unless it is an HTML page, which it logs); a body that is not one fails the way `readValue` does.
fn http_exception(url: &str, status: u16, content_type: &str, body: &str) -> String {
    let response = if content_type.starts_with("text/html") {
        log::error!("Got an error with a html body connecting to {url}: {body}");
        "null".to_owned()
    } else {
        match read_value(body, error_response) {
            Ok(response) => response.unwrap_or_else(|| "null".to_owned()),
            Err(exception) => return exception,
        }
    };
    format!("MinecraftClientHttpException[type=HTTP_ERROR, status={status}, response={response}]")
}

/// `MinecraftClient.get`: the JSON object of the body read by `read` (`None` for an empty one), or the first line of the
/// exception vanilla would throw.
fn get_json<T>(
    client: &reqwest::blocking::Client,
    url: &str,
    read: impl Fn(&Value) -> Result<T, Unread>,
) -> Result<Option<T>, String> {
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
    read_value(&body, read)
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
    let lists = match get_json(client, keys_url, |body| {
        key_lists(body).ok_or(Unread::Malformed)
    }) {
        Ok(Some(lists)) => lists,
        Ok(None) => return None,
        // authlib wraps what it can read; the null key's NullPointerException reaches `RetryableFetch` as it is
        Err(exception) if exception == NULL_MAP_KEY => {
            log_fetch_failure(&exception);
            return None;
        }
        Err(exception) => {
            log::error!("Failed to request Minecraft Services public key\n{exception}");
            return None;
        }
    };
    match parse_keys(lists) {
        Ok(keys) => Some(keys),
        Err(exception) => {
            log::error!("Received malformed Minecraft Services public key data\n{exception}");
            None
        }
    }
}

/// `Base64.getDecoder().decode`: the standard alphabet, padding optional but exact when present, no stray bits check.
fn java_base64(text: &str) -> Option<Vec<u8>> {
    const LENIENT: GeneralPurpose = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::RequireNone)
            .with_decode_allow_trailing_bits(true),
    );
    let body = text.trim_end_matches('=');
    let padding = text.len() - body.len();
    let shaped = match padding {
        0 => body.len() % 4 != 1,
        1 | 2 => text.len().is_multiple_of(4),
        _ => false,
    };
    if !shaped {
        return None;
    }
    LENIENT.decode(body).ok()
}

/// The `KeyData` entries of the key lists as Gson reads them, `profilePropertyKeys` first: the decoded `publicKey`, or
/// the null dereference `MinecraftServicesKeyInfo.parseList` throws for an entry that is `null` or has no key.
type KeyLists = [Vec<Result<Vec<u8>, &'static str>>; 2];

const NULL_ENTRY: &str =
    "java.lang.NullPointerException: Cannot read field \"publicKey\" because \"data\" is null";
const NULL_KEY: &str = "java.lang.NullPointerException: Cannot invoke \"java.nio.ByteBuffer.array()\" because \"data.publicKey\" is null";

/// `KeySetResponse` through Gson: `None` when a list is not an array, an entry not an object, or a `publicKey` not
/// base64 text (`ByteBufferTypeAdapter`: strings and numbers are read as text).
fn key_lists(body: &Value) -> Option<KeyLists> {
    let list = |name: &str| -> Option<Vec<Result<Vec<u8>, &'static str>>> {
        let entries = match body.get(name) {
            None | Some(Value::Null) => return Some(Vec::new()),
            Some(Value::Array(entries)) => entries,
            Some(_) => return None,
        };
        entries
            .iter()
            .map(|entry| match entry {
                Value::Null => Some(Err(NULL_ENTRY)),
                Value::Object(fields) => match fields.get("publicKey") {
                    None | Some(Value::Null) => Some(Err(NULL_KEY)),
                    Some(Value::String(text)) => java_base64(text).map(Ok),
                    Some(Value::Number(number)) => java_base64(&number.to_string()).map(Ok),
                    Some(_) => None,
                },
                _ => None,
            })
            .collect()
    };
    Some([list("profilePropertyKeys")?, list("playerCertificateKeys")?])
}

/// `MinecraftServicesKeyInfo.parseList` of both lists, the first malformed key throwing the exception whose first line
/// is returned. Only the certificate keys are kept: nothing here validates profile properties.
fn parse_keys(lists: KeyLists) -> Result<KeySet, String> {
    let mut certificate_keys = Vec::new();
    for (index, list) in lists.into_iter().enumerate() {
        for entry in list {
            let key = entry.map_err(str::to_owned).and_then(|der| {
                RsaPublicKey::from_public_key_der(&der).map_err(|_| {
                    "java.lang.IllegalArgumentException: Invalid Minecraft Services public key!"
                        .to_owned()
                })
            })?;
            if index == 1 {
                certificate_keys.push(VerifyingKey::new(key));
            }
        }
    }
    Ok(KeySet(certificate_keys))
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
        let any = |v: &Value| Ok(v.clone());
        assert_eq!(read_value("  ", any), Ok(None));
        assert_eq!(read_value("null", any), Ok(None));
        assert_eq!(
            read_value(r#"{"a":1}"#, any),
            Ok(Some(serde_json::json!({"a": 1})))
        );
        assert!(read_value("[1]", any).is_err());
        assert!(read_value("{", any).is_err());
        assert!(read_value(r#"{"a":1}"#, |_| Err::<(), _>(Unread::Malformed)).is_err());
    }

    #[test]
    fn doubles_print_like_java() {
        for (value, text) in [
            (1.0, "1.0"),
            (1.5, "1.5"),
            (100.0, "100.0"),
            (9_999_999.0, "9999999.0"),
            (1e7, "1.0E7"),
            (12_345_678.9, "1.23456789E7"),
            (0.001, "0.001"),
            (0.000_1, "1.0E-4"),
            (1.5e-7, "1.5E-7"),
            (1e21, "1.0E21"),
            (-2.5, "-2.5"),
            (0.0, "0.0"),
            (123_456.789, "123456.789"),
            (0.5, "0.5"),
            (0.02, "0.02"),
            (5e-324, "4.9E-324"),
        ] {
            assert_eq!(java_double(value), text);
        }
    }

    #[test]
    fn error_fields_take_what_gson_gives_a_record() {
        let read = |body: &str| read_value(body, error_response);
        assert_eq!(
            read(r#"{"path":5,"error":true,"errorMessage":1.5}"#),
            Ok(Some(
                "ErrorResponse[path=5, error=true, errorMessage=1.5, details=null]".to_owned()
            ))
        );
        assert!(read(r#"{"path":{}}"#).is_err());
        assert!(read(r#"{"error":[]}"#).is_err());
        assert!(read(r#"{"details":"text"}"#).is_err());
        assert_eq!(
            read(r#"{"details":[[null,1,2]]}"#),
            Err(NULL_MAP_KEY.to_owned())
        );
        assert!(read(r#"{"details":[[null]]}"#).is_err_and(|e| e != NULL_MAP_KEY));
        assert!(
            read(r#"{"details":{"a":[1,{"b":null}]}}"#)
                .expect("read")
                .expect("a record")
                .ends_with("details={a=[1.0, {b=null}]}]")
        );
    }

    #[test]
    fn key_lists_fail_where_gson_does() {
        let lists = |body: Value| key_lists(&body);
        let key = "AAAA";
        assert!(lists(serde_json::json!({"playerCertificateKeys": "k"})).is_none());
        assert!(lists(serde_json::json!({"playerCertificateKeys": {"publicKey": key}})).is_none());
        assert!(lists(serde_json::json!({"playerCertificateKeys": [5]})).is_none());
        assert!(lists(serde_json::json!({"profilePropertyKeys": "k"})).is_none());
        for bad in [
            serde_json::json!("!!!"),
            serde_json::json!("A"),
            serde_json::json!("AAAAA"),
            serde_json::json!("A==="),
            serde_json::json!(12345),
            serde_json::json!(true),
            serde_json::json!({}),
            serde_json::json!([]),
        ] {
            assert!(
                lists(serde_json::json!({"playerCertificateKeys": [{"publicKey": bad}]})).is_none(),
                "{bad}"
            );
        }
        let [profile, certificate] = lists(serde_json::json!({
            "playerCertificateKeys": [null, {}, {"publicKey": "AAA"}, {"publicKey": "AA=="}],
        }))
        .expect("a key set");
        assert!(profile.is_empty());
        assert_eq!(
            certificate,
            [Err(NULL_ENTRY), Err(NULL_KEY), Ok(vec![0, 0]), Ok(vec![0])]
        );
        assert!(lists(serde_json::json!({"playerCertificateKeys": null})).is_some());
    }

    #[test]
    fn the_first_malformed_key_names_the_failure_and_profile_keys_count_too() {
        let certificate =
            |entry: Result<Vec<u8>, &'static str>| parse_keys([Vec::new(), vec![entry]]);
        assert_eq!(certificate(Err(NULL_KEY)).err().as_deref(), Some(NULL_KEY));
        assert!(
            certificate(Ok(vec![0, 0]))
                .err()
                .is_some_and(|e| e.ends_with("Invalid Minecraft Services public key!"))
        );
        assert!(parse_keys([vec![Ok(vec![1])], Vec::new()]).is_err());
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
