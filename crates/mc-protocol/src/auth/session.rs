use std::time::Duration;

use serde_json::Value;
use uuid::Uuid;

use crate::codec::{GameProfile, Property};

/// authlib `MinecraftClient.CONNECT_TIMEOUT_MS` / `READ_TIMEOUT_MS`.
const TIMEOUT: Duration = Duration::from_millis(5000);

/// Vanilla `AuthenticationUnavailableException`: the session server could not be asked
/// (the login then ends with `multiplayer.disconnect.authservers_down`).
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("session server request failed: {0}")]
    Io(#[from] reqwest::Error),
    #[error("session server returned HTTP {0}")]
    Status(u16),
}

/// Client for the Yggdrasil session server's `hasJoined` endpoint.
#[derive(Debug, Clone)]
pub struct SessionClient {
    base_url: String,
    http: reqwest::Client,
}

impl Default for SessionClient {
    fn default() -> Self {
        Self::new("https://sessionserver.mojang.com")
    }
}

impl SessionClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(TIMEOUT)
            .read_timeout(TIMEOUT)
            .build()
            .expect("TLS backend initialisation");
        Self { base_url: base_url.into().trim_end_matches('/').to_owned(), http }
    }

    /// `SessionService.hasJoinedServer`. `Ok(None)` means the player is not verified: 204/empty body,
    /// any 4xx, or an unusable response body (vanilla treats all of these as a `null` result).
    /// Transport failures and 5xx are [`AuthError`]. The returned profile carries the *requested*
    /// `username` (vanilla ignores the name in the response), the response's id and properties.
    pub async fn has_joined(
        &self,
        username: &str,
        server_hash: &str,
        ip: Option<&str>,
    ) -> Result<Option<GameProfile>, AuthError> {
        // authlib builds the query from a HashMap; its iteration order for these keys is ip, serverId, username.
        let mut query = String::new();
        if let Some(ip) = ip {
            query.push_str(&format!("ip={}&", url_encode(ip)));
        }
        query.push_str(&format!("serverId={}&username={}", url_encode(server_hash), url_encode(username)));
        let url = format!("{}/session/minecraft/hasJoined?{query}", self.base_url);

        let response = self.http.get(url).send().await?;
        let status = response.status();
        if status.is_server_error() {
            return Err(AuthError::Status(status.as_u16()));
        }
        if !status.is_success() {
            return Ok(None);
        }
        let body = response.bytes().await?;
        Ok(parse_profile(&body, username))
    }
}

/// authlib `HasJoinedMinecraftServerResponse` + `PropertyMap.Serializer` (array form).
fn parse_profile(body: &[u8], username: &str) -> Option<GameProfile> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let id = Uuid::parse_str(json.get("id")?.as_str()?).ok()?;
    let properties = json
        .get("properties")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let p = p.as_object()?;
            Some(Property {
                name: p.get("name")?.as_str()?.to_owned(),
                value: p.get("value")?.as_str()?.to_owned(),
                signature: p.get("signature").and_then(Value::as_str).map(str::to_owned),
            })
        })
        .collect();
    Some(GameProfile { id, name: username.to_owned(), properties })
}

/// `java.net.URLEncoder.encode(s, UTF_8)` (application/x-www-form-urlencoded).
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'*' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
