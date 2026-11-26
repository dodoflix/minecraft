use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use uuid::Uuid;

use crate::codec::{GameProfile, Property};

/// authlib `MinecraftClient.CONNECT_TIMEOUT_MS` / `READ_TIMEOUT_MS`.
const TIMEOUT: Duration = Duration::from_millis(5000);

/// `MinecraftServicesEnvironment.PROD` discovery URL.
pub const PROD_DISCOVERY_URL: &str = "https://discovery.minecraftservices.com/minecraft/client";

/// Vanilla `AuthenticationUnavailableException`: the session server could not be asked
/// (the login then ends with `multiplayer.disconnect.authservers_down`).
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("session server request failed: {0}")]
    Io(#[from] reqwest::Error),
    #[error("session server returned HTTP {0}")]
    Status(u16),
    /// `MinecraftServicesDiscoveryService.getEndpoint`: "Services are unavailable".
    #[error("Services are unavailable")]
    ServicesUnavailable,
}

/// Where the `join` and `verify` endpoints come from.
#[derive(Debug, Clone)]
enum Endpoints {
    /// A session server at a fixed base URL (tests).
    Fixed(String),
    /// The services discovery document, fetched on first use.
    // shortcut: the document is never refreshed (vanilla refetches every 24 h), restart to pick up a change.
    Discovery {
        url: String,
        document: Arc<tokio::sync::Mutex<Fetched>>,
    },
}

/// The discovery document and the state of its fetch (`RetryableFetch`): a failed fetch is not repeated before
/// `BASE_FAILURE_INTERVAL_MINUTES` (1 for the discovery document) doubled per earlier failure, at most 2^6 times.
#[derive(Debug, Default)]
struct Fetched {
    document: Option<Value>,
    failures: u32,
    retry_at: Option<std::time::Instant>,
}

/// Client for the session server's `join` and `hasJoined` endpoints
/// (authlib `MinecraftServicesSessionService`).
#[derive(Debug, Clone)]
pub struct SessionClient {
    endpoints: Endpoints,
    http: reqwest::Client,
}

impl Default for SessionClient {
    fn default() -> Self {
        Self::from_discovery(PROD_DISCOVERY_URL)
    }
}

impl SessionClient {
    /// A session server at `base_url`, without a discovery document.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with(Endpoints::Fixed(
            base_url.into().trim_end_matches('/').to_owned(),
        ))
    }

    /// The endpoints named by the discovery document at `discovery_url`.
    pub fn from_discovery(discovery_url: impl Into<String>) -> Self {
        Self::with(Endpoints::Discovery {
            url: discovery_url.into(),
            document: Arc::default(),
        })
    }

    fn with(endpoints: Endpoints) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(TIMEOUT)
            .read_timeout(TIMEOUT)
            .build()
            .expect("TLS backend initialisation");
        Self { endpoints, http }
    }

    /// `MinecraftServicesDiscoveryService.getUrl(SESSION, key)`.
    async fn endpoint(&self, key: &str, fixed_path: &str) -> Result<String, AuthError> {
        let (url, document) = match &self.endpoints {
            Endpoints::Fixed(base) => return Ok(format!("{base}{fixed_path}")),
            Endpoints::Discovery { url, document } => (url, document),
        };
        let mut fetched = document.lock().await;
        let due = fetched
            .retry_at
            .is_none_or(|at| std::time::Instant::now() >= at);
        if fetched.document.is_none() && due {
            let response = async {
                self.http
                    .get(url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await
            }
            .await;
            match response {
                Ok(document) => {
                    *fetched = Fetched {
                        document: Some(document),
                        ..Fetched::default()
                    }
                }
                Err(e) => {
                    log::warn!("Failed to connect: {e}");
                    let exponent = fetched.failures.min(6);
                    fetched.failures += 1;
                    fetched.retry_at =
                        Some(std::time::Instant::now() + Duration::from_secs(60 * (1 << exponent)));
                }
            }
        }
        let Some(document) = &fetched.document else {
            log::warn!("Services are unavailable, unable to fetch uri for SESSION/{key}");
            return Err(AuthError::ServicesUnavailable);
        };
        match document
            .pointer(&format!("/discovery/session/endpoints/{key}/uri"))
            .and_then(Value::as_str)
        {
            Some(uri) => Ok(uri.to_owned()),
            None => {
                log::warn!("Services are available, but unable to find uri for SESSION/{key}");
                Err(AuthError::ServicesUnavailable)
            }
        }
    }

    /// `SessionService.joinServer`: the client tells the session server it is joining `server_id`.
    /// Any non-success status is an error (vanilla `AuthenticationException`).
    pub async fn join_server(
        &self,
        profile_id: Uuid,
        access_token: &str,
        server_id: &str,
    ) -> Result<(), AuthError> {
        let url = self.endpoint("join", "/session/minecraft/join").await?;
        let body = serde_json::json!({
            "accessToken": access_token,
            "selectedProfile": profile_id.simple().to_string(),
            "serverId": server_id,
        });
        let status = self.http.post(url).json(&body).send().await?.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(AuthError::Status(status.as_u16()))
        }
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
        query.push_str(&format!(
            "serverId={}&username={}",
            url_encode(server_hash),
            url_encode(username)
        ));
        // `HttpDiscoveryService.concatenateURL`
        let base = self
            .endpoint("verify", "/session/minecraft/hasJoined")
            .await?;
        let separator = if base.split_once('?').is_some_and(|(_, q)| !q.is_empty()) {
            '&'
        } else {
            '?'
        };
        let url = format!("{base}{separator}{query}");

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
                signature: p
                    .get("signature")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect();
    Some(GameProfile {
        id,
        name: username.to_owned(),
        properties,
    })
}

/// `java.net.URLEncoder.encode(s, UTF_8)` (application/x-www-form-urlencoded).
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'*' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
