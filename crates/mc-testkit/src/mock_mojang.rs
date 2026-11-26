//! A local stand-in for the Mojang web services, for tests and the real-client end-to-end runs that must not reach
//! the real ones.
//!
//! authlib (client and vanilla server alike) takes every service URL from one discovery document. Setting the JVM
//! property `minecraft.api.discovery.host` to [`MockMojang::discovery_url`] (see [`crate::Builder::mojang`]) points all of
//! them here. Our own server reads `MINECRAFT_API_DISCOVERY_HOST` for its profile key set, profile lookups and
//! `hasJoined` requests. The mock serves:
//!
//! | Discovery entry | Route | Answer |
//! |---|---|---|
//! | `session.join` | `POST /session/minecraft/join` | 204, the join is recorded |
//! | `session.verify` | `GET /session/minecraft/hasJoined?username&serverId[&ip]` | the profile, or 204 without a matching join |
//! | `session.getProfileById` | `GET /session/minecraft/profile/{id}` | the profile of a name the mock has seen, else 204 |
//! | `authentication.getPublicKeys` | `GET /publickeys` | the mock's own signing key as profile property and player certificate key |
//! | `player.getCertificates` | `POST /player/certificates` | a key pair for the token's profile, certified by the mock key |
//! | `player.getAttributes` | `GET /player/attributes` | every privilege granted, no ban |
//! | `player.getBlocklist` | `GET /privacy/blocklist` | nobody blocked |
//!
//! Identity: the bearer token of a request is the profile's identity. A token that parses as a UUID is that profile id;
//! any other token is a player name whose id is the offline-mode UUID (`OfflinePlayer:<name>`), which is what a
//! client started with `--username NAME --uuid <offline uuid> --accessToken NAME` has. A `hasJoined` for a name
//! succeeds when a recorded join for the same server id names that name's profile (an id registered with
//! [`MockMojang::register_profile`], else the offline one).
//!
//! The mock has its own 2048-bit RSA key (the real service's is 4096 bits; the verifiers do not look at the size).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use md5::{Digest as _, Md5};
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
use rsa::signature::{SignatureEncoding, Signer};
use serde_json::{Value, json};
use sha1::Sha1;
use uuid::Uuid;

use crate::lock;

/// How long a player certificate lives, as in the real service.
const CERTIFICATE_LIFETIME_MS: i64 = 48 * 3_600_000;
/// When a client should fetch a new certificate, as in the real service.
const CERTIFICATE_REFRESH_MS: i64 = 24 * 3_600_000;

/// One request the mock received, in arrival order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// Path and query exactly as sent.
    pub target: String,
    /// The `Authorization` header, for example `Bearer Steve`.
    pub authorization: Option<String>,
    pub body: String,
}

/// A recorded `session.join` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Join {
    pub profile_id: Uuid,
    pub server_id: String,
    pub access_token: String,
}

/// The key pair and certificate the mock hands a profile.
#[derive(Clone, Debug)]
pub struct Certificate {
    pub profile_id: Uuid,
    /// PKCS#8 DER of the private key.
    pub private_key_der: Vec<u8>,
    /// X.509 `SubjectPublicKeyInfo` DER of the public key.
    pub public_key_der: Vec<u8>,
    pub expires_at_millis: i64,
    /// `publicKeySignatureV2`: SHA1withRSA by the mock key over profile id, `expires_at_millis` and the public key.
    pub signature: Vec<u8>,
}

#[derive(Default)]
struct State {
    requests: Vec<Request>,
    joins: Vec<Join>,
    /// Lower-case player name to the name as spelled and the profile id.
    profiles: HashMap<String, (String, Uuid)>,
    certificates: HashMap<Uuid, Certificate>,
    /// Answers that replace a route's own: path to status and JSON body.
    overrides: HashMap<String, (u16, Value)>,
}

struct Shared {
    base: String,
    key: SigningKey<Sha1>,
    public_key_der: Vec<u8>,
    state: Mutex<State>,
    sink: Option<Mutex<File>>,
    stop: AtomicBool,
}

/// A running mock. Dropping it stops the listener.
pub struct MockMojang {
    shared: Arc<Shared>,
    addr: SocketAddr,
}

impl MockMojang {
    /// Starts on a free loopback port.
    pub fn start() -> io::Result<Self> {
        Self::bind(0, None)
    }

    /// Starts on `port` (0: any free one); every request is also appended to `log` as one JSON object per line.
    pub fn bind(port: u16, log: Option<&Path>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let addr = listener.local_addr()?;
        let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
            .map_err(|e| io::Error::other(format!("generate the mock key: {e}")))?;
        let public_key_der = spki_der(&private)?;
        let sink = match log {
            Some(path) => Some(Mutex::new(
                OpenOptions::new().create(true).append(true).open(path)?,
            )),
            None => None,
        };
        let shared = Arc::new(Shared {
            base: format!("http://{addr}"),
            key: SigningKey::new(private),
            public_key_der,
            state: Mutex::default(),
            sink,
            stop: AtomicBool::new(false),
        });
        let served = shared.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if served.stop.load(Ordering::Acquire) {
                    return;
                }
                if let Ok(stream) = stream {
                    let conn = served.clone();
                    std::thread::spawn(move || {
                        let _ = serve(&conn, stream);
                    });
                }
            }
        });
        Ok(MockMojang { shared, addr })
    }

    /// `http://127.0.0.1:PORT`.
    pub fn base_url(&self) -> &str {
        &self.shared.base
    }

    /// The value of `-Dminecraft.api.discovery.host` / `MINECRAFT_API_DISCOVERY_HOST`.
    pub fn discovery_url(&self) -> String {
        format!("{}/minecraft/client", self.shared.base)
    }

    /// DER of the mock's public key, the one a verifier must trust for profile keys.
    pub fn services_public_key_der(&self) -> &[u8] {
        &self.shared.public_key_der
    }

    /// Every request so far.
    pub fn requests(&self) -> Vec<Request> {
        lock(&self.shared.state).requests.clone()
    }

    /// Every `session.join` so far.
    pub fn joins(&self) -> Vec<Join> {
        lock(&self.shared.state).joins.clone()
    }

    /// Makes `hasJoined` answer for `name` with `id` instead of the offline UUID.
    pub fn register_profile(&self, name: &str, id: Uuid) {
        lock(&self.shared.state)
            .profiles
            .insert(name.to_lowercase(), (name.to_owned(), id));
    }

    /// Makes `path` (for example `/minecraft/client` or `/publickeys`) answer `status` with the JSON `body` from now on,
    /// whatever the route would have said: a service that is up but failing.
    pub fn respond(&self, path: &str, status: u16, body: Value) {
        lock(&self.shared.state)
            .overrides
            .insert(path.to_owned(), (status, body));
    }

    /// Re-issues the certificate of `profile_id` (same key pair) to expire at `expires_at_millis` and signs it again; the
    /// next `POST /player/certificates` of that profile serves it. An expiry in the past gives a key vanilla rejects.
    pub fn set_certificate_expiry(
        &self,
        profile_id: Uuid,
        expires_at_millis: i64,
    ) -> io::Result<Certificate> {
        let mut cert = certificate(&self.shared, profile_id)?;
        cert.expires_at_millis = expires_at_millis;
        cert.signature = sign_certificate(
            &self.shared.key,
            profile_id,
            expires_at_millis,
            &cert.public_key_der,
        );
        lock(&self.shared.state)
            .certificates
            .insert(profile_id, cert.clone());
        Ok(cert)
    }

    /// The certificate served to `profile_id` (generated on first use, the same one afterwards).
    pub fn certificate(&self, profile_id: Uuid) -> io::Result<Certificate> {
        certificate(&self.shared, profile_id)
    }
}

impl Drop for MockMojang {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        // wakes the accept loop so it sees the flag
        let _ = TcpStream::connect(self.addr);
    }
}

/// The version-3 UUID of `OfflinePlayer:<name>` (`UUIDUtil.createOfflinePlayerUUID`).
pub fn offline_uuid(name: &str) -> Uuid {
    let hash = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    uuid::Builder::from_md5_bytes(hash.into()).into_uuid()
}

fn spki_der(private: &RsaPrivateKey) -> io::Result<Vec<u8>> {
    Ok(private
        .to_public_key()
        .to_public_key_der()
        .map_err(|e| io::Error::other(format!("encode a public key: {e}")))?
        .as_bytes()
        .to_vec())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// `Instant.toString()` for whole milliseconds: `2026-11-26T12:00:00.123Z`, no fraction at 0 ms.
fn iso_instant(millis: i64) -> String {
    let secs = millis.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let fraction = match millis.rem_euclid(1000) {
        0 => String::new(),
        ms => format!(".{ms:03}"),
    };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{fraction}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// PEM as `Crypt.rsaPrivateKeyToString` writes it (the label says RSA, the body is PKCS#8).
fn pem(label: &str, der: &[u8]) -> String {
    let body = B64.encode(der);
    let lines: Vec<&str> = body
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect();
    format!(
        "-----BEGIN RSA {label} KEY-----\n{}\n-----END RSA {label} KEY-----\n",
        lines.join("\n")
    )
}

/// `publicKeySignatureV2`: the services key's SHA1withRSA over profile id, expiry (milliseconds) and the player's key.
fn sign_certificate(
    key: &SigningKey<Sha1>,
    profile_id: Uuid,
    expires_at_millis: i64,
    public_key_der: &[u8],
) -> Vec<u8> {
    let mut payload = profile_id.as_bytes().to_vec();
    payload.extend_from_slice(&expires_at_millis.to_be_bytes());
    payload.extend_from_slice(public_key_der);
    key.sign(&payload).to_vec()
}

fn certificate(shared: &Shared, profile_id: Uuid) -> io::Result<Certificate> {
    if let Some(c) = lock(&shared.state).certificates.get(&profile_id) {
        return Ok(c.clone());
    }
    // generated outside the lock: a few hundred ms to seconds, other requests keep being served
    let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
        .map_err(|e| io::Error::other(format!("generate a player key: {e}")))?;
    let public_key_der = spki_der(&private)?;
    let expires_at_millis = now_millis() / 1000 * 1000 + CERTIFICATE_LIFETIME_MS;
    let signature = sign_certificate(&shared.key, profile_id, expires_at_millis, &public_key_der);
    let made = Certificate {
        profile_id,
        private_key_der: private
            .to_pkcs8_der()
            .map_err(|e| io::Error::other(format!("encode a private key: {e}")))?
            .as_bytes()
            .to_vec(),
        public_key_der,
        expires_at_millis,
        signature,
    };
    Ok(lock(&shared.state)
        .certificates
        .entry(profile_id)
        .or_insert(made)
        .clone())
}

// ---- HTTP -------------------------------------------------------------------------------------------------------

struct Response {
    status: u16,
    body: Option<Value>,
}

impl Response {
    fn json(body: Value) -> Self {
        Response {
            status: 200,
            body: Some(body),
        }
    }

    fn empty(status: u16) -> Self {
        Response { status, body: None }
    }

    fn error(status: u16, path: &str, message: &str) -> Self {
        Response {
            status,
            body: Some(json!({"path": path, "errorMessage": message})),
        }
    }
}

fn serve(shared: &Shared, stream: TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(()); // the wake-up connection of `Drop`, or garbage
    };
    let (method, target) = (method.to_owned(), target.to_owned());
    let (mut length, mut authorization) = (0usize, None);
    loop {
        line.clear();
        reader.read_line(&mut line)?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "authorization" => authorization = Some(value.trim().to_owned()),
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let request = Request {
        method,
        target,
        authorization,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    if let Some(sink) = &shared.sink {
        let entry = json!({
            "method": request.method, "target": request.target,
            "authorization": request.authorization, "body": request.body,
        });
        let _ = writeln!(lock(sink), "{entry}");
    }
    lock(&shared.state).requests.push(request.clone());
    let response = route(shared, &request);
    let mut out = stream;
    let text = response.body.map(|b| b.to_string());
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        _ => "Error",
    };
    write!(
        out,
        "HTTP/1.1 {} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        text.as_ref().map_or(0, String::len),
        text.unwrap_or_default()
    )?;
    out.flush()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match b[i] {
            b'%' if i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some() => {
                out.push((hex(b[i + 1]).unwrap_or(0) * 16 + hex(b[i + 2]).unwrap_or(0)) as u8);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn undashed(id: Uuid) -> String {
    id.simple().to_string()
}

/// The profile a bearer token stands for, see the module docs.
fn token_profile(token: &str) -> Uuid {
    Uuid::parse_str(token).unwrap_or_else(|_| offline_uuid(token))
}

fn profile_of(shared: &Shared, name: &str) -> Uuid {
    lock(&shared.state)
        .profiles
        .get(&name.to_lowercase())
        .map_or_else(|| offline_uuid(name), |(_, id)| *id)
}

fn route(shared: &Shared, request: &Request) -> Response {
    let (path, query) = request
        .target
        .split_once('?')
        .unwrap_or((&request.target, ""));
    let param = |key: &str| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('=').or(Some((kv, ""))))
            .find(|(k, _)| percent_decode(k) == key)
            .map(|(_, v)| percent_decode(v))
    };
    if let Some((status, body)) = lock(&shared.state).overrides.get(path).cloned() {
        return Response {
            status,
            body: Some(body),
        };
    }
    let base = &shared.base;
    match (request.method.as_str(), path) {
        ("GET", "/minecraft/client") => Response::json(discovery(base)),
        ("GET", "/publickeys") => {
            let key = json!([{"publicKey": B64.encode(&shared.public_key_der)}]);
            Response::json(json!({"profilePropertyKeys": key, "playerCertificateKeys": key}))
        }
        ("POST", "/session/minecraft/join") => {
            let Ok(body) = serde_json::from_str::<Value>(&request.body) else {
                return Response::error(400, path, "malformed join request");
            };
            let text = |key: &str| body.get(key).and_then(Value::as_str);
            let (Some(token), Some(profile), Some(server_id)) = (
                text("accessToken"),
                text("selectedProfile").and_then(|p| Uuid::parse_str(p).ok()),
                text("serverId"),
            ) else {
                return Response::error(400, path, "incomplete join request");
            };
            lock(&shared.state).joins.push(Join {
                profile_id: profile,
                server_id: server_id.to_owned(),
                access_token: token.to_owned(),
            });
            Response::empty(204)
        }
        ("GET", "/session/minecraft/hasJoined") => {
            let (Some(name), Some(server_id)) = (param("username"), param("serverId")) else {
                return Response::empty(204);
            };
            let id = profile_of(shared, &name);
            let mut state = lock(&shared.state);
            if !state
                .joins
                .iter()
                .any(|j| j.server_id == server_id && j.profile_id == id)
            {
                return Response::empty(204);
            }
            state
                .profiles
                .entry(name.to_lowercase())
                .or_insert((name.clone(), id));
            Response::json(json!({"id": undashed(id), "name": name, "properties": []}))
        }
        ("GET", p) if p.starts_with("/session/minecraft/profile/") => {
            let Ok(id) = Uuid::parse_str(&p["/session/minecraft/profile/".len()..]) else {
                return Response::error(400, path, "malformed profile id");
            };
            let state = lock(&shared.state);
            match state.profiles.values().find(|(_, v)| *v == id) {
                Some((name, _)) => {
                    Response::json(json!({"id": undashed(id), "name": name, "properties": []}))
                }
                None => Response::empty(204),
            }
        }
        ("POST", "/player/certificates") => {
            let token = request
                .authorization
                .as_deref()
                .and_then(|a| a.strip_prefix("Bearer "));
            let Some(token) = token else {
                return Response::error(401, path, "missing bearer token");
            };
            match certificate(shared, token_profile(token)) {
                Ok(c) => Response::json(json!({
                    "keyPair": {
                        "privateKey": pem("PRIVATE", &c.private_key_der),
                        "publicKey": pem("PUBLIC", &c.public_key_der),
                    },
                    "publicKeySignatureV2": B64.encode(&c.signature),
                    "expiresAt": iso_instant(c.expires_at_millis),
                    "refreshedAfter": iso_instant(c.expires_at_millis - CERTIFICATE_LIFETIME_MS + CERTIFICATE_REFRESH_MS),
                })),
                Err(e) => Response::error(500, path, &e.to_string()),
            }
        }
        ("GET", "/player/attributes") => Response::json(json!({
            "privileges": {
                "onlineChat": {"enabled": true},
                "multiplayerServer": {"enabled": true},
                "multiplayerRealms": {"enabled": false},
                "telemetry": {"enabled": false},
                "optionalTelemetry": {"enabled": false},
            },
            "profanityFilterPreferences": {"profanityFilterOn": false},
            "banStatus": {"bannedScopes": {}},
        })),
        ("GET", "/privacy/blocklist") => Response::json(json!({"blockedProfiles": []})),
        _ => Response::error(404, path, "not found"),
    }
}

fn discovery(base: &str) -> Value {
    let endpoint = |path: &str| json!({"uri": format!("{base}{path}"), "validUris": []});
    json!({
        "environment": "mock",
        "product": "minecraft",
        "discovery": {
            "product": "minecraft",
            "authentication": {"endpoints": {"getPublicKeys": endpoint("/publickeys")}},
            "session": {"endpoints": {
                "join": endpoint("/session/minecraft/join"),
                "verify": endpoint("/session/minecraft/hasJoined"),
                "getProfileById": endpoint("/session/minecraft/profile/{profileId}"),
            }},
            "player": {"endpoints": {
                "getCertificates": endpoint("/player/certificates"),
                "getAttributes": endpoint("/player/attributes"),
                "getBlocklist": endpoint("/privacy/blocklist"),
            }},
            "profiles": {"endpoints": {"getTexture": {
                "uri": format!("{base}/textures/{{textureId}}"),
                "validUris": [format!("{base}/textures/{{textureId}}")],
            }}},
            "telemetry": {"endpoints": {}},
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::RsaPublicKey;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::pkcs8::DecodePublicKey;
    use rsa::signature::Verifier;

    /// One request over a fresh connection: status and body text.
    fn http(
        mock: &MockMojang,
        method: &str,
        target: &str,
        token: Option<&str>,
        body: &str,
    ) -> (u16, String) {
        let mut s = TcpStream::connect(mock.addr).expect("connect");
        let auth = token.map_or(String::new(), |t| format!("Authorization: Bearer {t}\r\n"));
        write!(
            s,
            "{method} {target} HTTP/1.1\r\nHost: x\r\n{auth}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("send");
        let mut text = String::new();
        s.read_to_string(&mut text).expect("read");
        let status = text[9..12].parse().expect("status");
        let body = text
            .split_once("\r\n\r\n")
            .map_or("", |(_, b)| b)
            .to_owned();
        (status, body)
    }

    fn json_of(mock: &MockMojang, method: &str, target: &str, token: Option<&str>) -> Value {
        let (status, body) = http(mock, method, target, token, "");
        assert_eq!(status, 200, "{target}: {body}");
        serde_json::from_str(&body).expect("json body")
    }

    #[test]
    fn offline_uuid_matches_vanilla() {
        assert_eq!(
            offline_uuid("Steve").to_string(),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
    }

    #[test]
    fn instants_print_like_java() {
        assert_eq!(iso_instant(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_instant(951_782_400_123), "2000-02-29T00:00:00.123Z");
        assert_eq!(iso_instant(1_795_694_400_000), "2026-11-26T12:00:00Z");
    }

    #[test]
    fn discovery_lists_every_endpoint_under_the_mock() {
        let mock = MockMojang::start().unwrap();
        let doc = json_of(&mock, "GET", "/minecraft/client", None);
        assert_eq!(doc["environment"], "mock");
        let uri = |service: &str, key: &str| {
            doc["discovery"][service]["endpoints"][key]["uri"]
                .as_str()
                .unwrap_or_else(|| panic!("{service}.{key}"))
                .to_owned()
        };
        let base = mock.base_url();
        assert_eq!(
            uri("authentication", "getPublicKeys"),
            format!("{base}/publickeys")
        );
        assert_eq!(
            uri("session", "join"),
            format!("{base}/session/minecraft/join")
        );
        assert_eq!(
            uri("session", "verify"),
            format!("{base}/session/minecraft/hasJoined")
        );
        assert!(uri("session", "getProfileById").ends_with("/profile/{profileId}"));
        for key in ["getCertificates", "getAttributes", "getBlocklist"] {
            assert!(uri("player", key).starts_with(base), "{key}");
        }
        assert!(
            doc["discovery"]["profiles"]["endpoints"]["getTexture"]["validUris"][0].is_string()
        );
        assert_eq!(mock.discovery_url(), format!("{base}/minecraft/client"));
    }

    #[test]
    fn public_keys_serve_the_signing_key_for_both_key_types() {
        let mock = MockMojang::start().unwrap();
        let keys = json_of(&mock, "GET", "/publickeys", None);
        for set in ["profilePropertyKeys", "playerCertificateKeys"] {
            let der = B64
                .decode(keys[set][0]["publicKey"].as_str().unwrap())
                .unwrap();
            assert_eq!(der, mock.services_public_key_der(), "{set}");
        }
    }

    #[test]
    fn a_join_makes_has_joined_return_the_profile() {
        let mock = MockMojang::start().unwrap();
        let id = offline_uuid("Steve");
        let join = format!(
            r#"{{"accessToken":"Steve","selectedProfile":"{}","serverId":"-1a2b"}}"#,
            undashed(id)
        );
        // nothing joined yet
        let query = "/session/minecraft/hasJoined?username=Steve&serverId=-1a2b&ip=127.0.0.1";
        assert_eq!(http(&mock, "GET", query, None, "").0, 204);
        assert_eq!(
            http(
                &mock,
                "POST",
                "/session/minecraft/join",
                Some("Steve"),
                &join
            )
            .0,
            204
        );
        let profile = json_of(&mock, "GET", query, None);
        assert_eq!(
            profile,
            json!({"id": undashed(id), "name": "Steve", "properties": []})
        );
        // another server id, another name
        let other = "/session/minecraft/hasJoined?username=Steve&serverId=2c";
        assert_eq!(http(&mock, "GET", other, None, "").0, 204);
        let alex = "/session/minecraft/hasJoined?username=Alex&serverId=-1a2b";
        assert_eq!(http(&mock, "GET", alex, None, "").0, 204);
        assert_eq!(
            mock.joins(),
            [Join {
                profile_id: id,
                server_id: "-1a2b".into(),
                access_token: "Steve".into()
            }]
        );
    }

    #[test]
    fn a_registered_profile_replaces_the_offline_id() {
        let mock = MockMojang::start().unwrap();
        let id = Uuid::from_u128(0x1234);
        mock.register_profile("Steve", id);
        let join = format!(r#"{{"accessToken":"t","selectedProfile":"{id}","serverId":"7"}}"#);
        http(&mock, "POST", "/session/minecraft/join", None, &join);
        let query = "/session/minecraft/hasJoined?username=steve&serverId=7";
        assert_eq!(json_of(&mock, "GET", query, None)["id"], undashed(id));
        // the profile lookup by id answers for a name the mock knows
        let by_id = json_of(
            &mock,
            "GET",
            &format!("/session/minecraft/profile/{}?unsigned=true", undashed(id)),
            None,
        );
        assert_eq!(by_id["name"], "Steve"); // the registered spelling, not the lower-case key
        let unknown = format!(
            "/session/minecraft/profile/{}",
            undashed(Uuid::from_u128(9))
        );
        assert_eq!(http(&mock, "GET", &unknown, None, "").0, 204);
    }

    #[test]
    fn a_malformed_join_is_rejected() {
        let mock = MockMojang::start().unwrap();
        assert_eq!(
            http(&mock, "POST", "/session/minecraft/join", None, "{").0,
            400
        );
        assert_eq!(
            http(
                &mock,
                "POST",
                "/session/minecraft/join",
                None,
                r#"{"serverId":"1"}"#
            )
            .0,
            400
        );
        assert!(mock.joins().is_empty());
    }

    #[test]
    fn certificates_are_signed_by_the_mock_key_for_the_token_profile() {
        let mock = MockMojang::start().unwrap();
        let response = json_of(&mock, "POST", "/player/certificates", Some("Steve"));
        let id = offline_uuid("Steve");
        let cert = mock.certificate(id).unwrap();
        // PEM: label, 76 column lines, the DER round-trips
        let pair = &response["keyPair"];
        let public = pair["publicKey"].as_str().unwrap();
        assert!(
            public.starts_with("-----BEGIN RSA PUBLIC KEY-----\n"),
            "{public}"
        );
        assert!(public.ends_with("\n-----END RSA PUBLIC KEY-----\n"));
        let unpem = |text: &str| {
            let body: String = text.lines().filter(|l| !l.starts_with("-----")).collect();
            B64.decode(body).unwrap()
        };
        assert_eq!(unpem(public), cert.public_key_der);
        assert_eq!(
            unpem(pair["privateKey"].as_str().unwrap()),
            cert.private_key_der
        );
        assert!(
            pair["privateKey"]
                .as_str()
                .unwrap()
                .starts_with("-----BEGIN RSA PRIVATE KEY-----\n")
        );
        // the signature covers profile id, expiry and key, and verifies with the published key
        let mut payload = id.as_bytes().to_vec();
        payload.extend_from_slice(&cert.expires_at_millis.to_be_bytes());
        payload.extend_from_slice(&cert.public_key_der);
        let signature = B64
            .decode(response["publicKeySignatureV2"].as_str().unwrap())
            .unwrap();
        let services = RsaPublicKey::from_public_key_der(mock.services_public_key_der()).unwrap();
        VerifyingKey::<Sha1>::new(services)
            .verify(
                &payload,
                &Signature::try_from(signature.as_slice()).unwrap(),
            )
            .expect("signature of the certificate");
        assert_eq!(response["expiresAt"], iso_instant(cert.expires_at_millis));
        assert!(
            response["refreshedAfter"].as_str().unwrap() < response["expiresAt"].as_str().unwrap()
        );
        // the same key the next time, and a UUID token names the profile directly
        let again = json_of(&mock, "POST", "/player/certificates", Some("Steve"));
        assert_eq!(again["keyPair"]["publicKey"], pair["publicKey"]);
        let other = Uuid::from_u128(5);
        json_of(
            &mock,
            "POST",
            "/player/certificates",
            Some(&other.to_string()),
        );
        assert_eq!(mock.certificate(other).unwrap().profile_id, other);
        assert_eq!(http(&mock, "POST", "/player/certificates", None, "").0, 401);
    }

    #[test]
    fn attributes_and_blocklist_have_the_shapes_the_client_reads() {
        let mock = MockMojang::start().unwrap();
        let a = json_of(&mock, "GET", "/player/attributes", Some("Steve"));
        assert_eq!(a["privileges"]["onlineChat"]["enabled"], true);
        assert_eq!(a["privileges"]["multiplayerServer"]["enabled"], true);
        assert_eq!(a["profanityFilterPreferences"]["profanityFilterOn"], false);
        assert_eq!(a["banStatus"]["bannedScopes"], json!({}));
        let b = json_of(&mock, "GET", "/privacy/blocklist", Some("Steve"));
        assert_eq!(b["blockedProfiles"], json!([]));
    }

    #[test]
    fn unknown_routes_are_404_and_every_request_is_logged() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("requests.jsonl");
        let mock = MockMojang::bind(0, Some(&log)).unwrap();
        let (status, body) = http(&mock, "GET", "/nope?x=1", Some("T"), "");
        assert_eq!(status, 404);
        assert!(body.contains("not found"), "{body}");
        let seen = mock.requests();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].target, "/nope?x=1");
        assert_eq!(seen[0].authorization.as_deref(), Some("Bearer T"));
        let line: Value =
            serde_json::from_str(std::fs::read_to_string(&log).unwrap().trim()).unwrap();
        assert_eq!(line["method"], "GET");
        assert_eq!(line["target"], "/nope?x=1");
    }

    #[test]
    fn a_certificate_can_be_reissued_with_another_expiry() {
        let mock = MockMojang::start().unwrap();
        let id = offline_uuid("Steve");
        let first = mock.certificate(id).unwrap();
        let past = now_millis() - 3_600_000;
        let second = mock.set_certificate_expiry(id, past).unwrap();
        assert_eq!(second.public_key_der, first.public_key_der);
        assert_ne!(second.signature, first.signature);
        let served = json_of(&mock, "POST", "/player/certificates", Some("Steve"));
        assert_eq!(served["expiresAt"], iso_instant(past));
        let signature = B64
            .decode(served["publicKeySignatureV2"].as_str().unwrap())
            .unwrap();
        let mut payload = id.as_bytes().to_vec();
        payload.extend_from_slice(&past.to_be_bytes());
        payload.extend_from_slice(&second.public_key_der);
        let services = RsaPublicKey::from_public_key_der(mock.services_public_key_der()).unwrap();
        VerifyingKey::<Sha1>::new(services)
            .verify(
                &payload,
                &Signature::try_from(signature.as_slice()).unwrap(),
            )
            .expect("signature over the new expiry");
    }

    #[test]
    fn a_route_can_be_made_to_fail() {
        let mock = MockMojang::start().unwrap();
        mock.respond("/publickeys", 500, json!({"errorMessage": "down"}));
        let (status, body) = http(&mock, "GET", "/publickeys", None, "");
        assert_eq!(status, 500);
        assert!(body.contains("down"), "{body}");
        // the other routes are untouched
        assert_eq!(http(&mock, "GET", "/minecraft/client", None, "").0, 200);
    }

    #[test]
    fn percent_escapes_decode() {
        assert_eq!(percent_decode("a%2Db+c%zz%4"), "a-b c%zz%4");
    }
}
