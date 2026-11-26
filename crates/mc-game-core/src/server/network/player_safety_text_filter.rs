//! `net.minecraft.server.network.PlayerSafetyServiceTextFilter`: text filtering version 1 (`text-filtering-config`
//! with an Azure AD application whose certificate authenticates the requests to the Player Safety service).
//!
//! The `ConfidentialClientApplication` of MSAL4J is reduced to what vanilla uses of it: the client credentials flow
//! with a certificate (`sendX5c`), an access token cached until five minutes before it expires.

use super::legacy_text_filter::{parse_config_object, uri};
use super::server_text_filter::{
    FilterBackend, FilterError, IgnoreStrategy, MessageEncoder, ServerTextFilter, parse_mask,
};
use super::text_filter_json::{
    as_object, convert_to_string, get_as_array, get_as_array_or_empty, get_as_bool_or,
    get_as_int_or, get_as_object_opt, get_as_string, get_as_string_or,
};
use crate::server::chat::FilteredText;
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use mc_core::gson::JsonObject;
use mc_core::java_exception::JavaException;
use mc_protocol::codec::GameProfile;
use parking_lot::Mutex;
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Test seam: the scheme and host of `https://login.microsoftonline.com`, replaced for discovery and token requests.
const AUTHORITY_HOST_ENV: &str = "MC_TEXT_FILTER_AUTHORITY_HOST";
const DEFAULT_AUTHORITY_HOST: &str = "https://login.microsoftonline.com";

const MSAL_SERVICE_EXCEPTION: &str = "com.microsoft.aad.msal4j.MsalServiceException";
const MSAL_CLIENT_EXCEPTION: &str = "com.microsoft.aad.msal4j.MsalClientException";
const JWT_BEARER: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
/// `AAD_JWT_TOKEN_LIFETIME_SECONDS`.
const ASSERTION_LIFETIME_SECONDS: u64 = 10 * 60;
/// MSAL refreshes a token that expires within five minutes.
const REFRESH_MARGIN: Duration = Duration::from_secs(5 * 60);

pub struct PlayerSafetyServiceTextFilter {
    client: ConfidentialClient,
    fully_filtered_events: HashSet<String>,
    connection_read_timeout_ms: i32,
}

/// `createTextFilterFromConfig`. Unlike version 0 nothing here is caught: a config that does not parse is an `Err` the
/// caller treats as fatal; an unreadable certificate or an authority MSAL rejects is a warning and no filter.
pub fn create_text_filter_from_config(
    config: &str,
) -> Result<Option<ServerTextFilter>, FilterError> {
    let parsed = parse_config_object(config)?;
    let host = uri(&get_as_string(&parsed, "apiServer")?)?;
    let api_path = get_as_string(&parsed, "apiPath")?;
    let scope = get_as_string(&parsed, "scope")?;
    let server_id = get_as_string_or(&parsed, "serverId", "")?;
    let application_id = get_as_string(&parsed, "applicationId")?;
    let tenant_id = get_as_string(&parsed, "tenantId")?;
    let room_id = get_as_string_or(&parsed, "roomId", "Java:Chat")?;
    let certificate_path = get_as_string(&parsed, "certificatePath")?;
    let certificate_password = get_as_string_or(&parsed, "certificatePassword", "")?;
    let hashes_to_drop = get_as_int_or(&parsed, "hashesToDrop", -1)?;
    let max_concurrent_requests = get_as_int_or(&parsed, "maxConcurrentRequests", 7)?;
    let mut fully_filtered_events = HashSet::new();
    for element in get_as_array(&parsed, "fullyFilteredEvents")? {
        fully_filtered_events.insert(convert_to_string(element, "filteredEvent")?);
    }
    let connection_read_timeout_ms = get_as_int_or(&parsed, "connectionReadTimeoutMs", 2000)?;

    // `host.resolve(apiPath).toURL()`
    let chat_endpoint = host.join(&api_path).map_err(|e| {
        JavaException::unchecked("java.lang.IllegalArgumentException", e.to_string())
    })?;
    let chat_encoder: MessageEncoder = Box::new(move |sender: &GameProfile, message: &str| {
        let mut object = JsonObject::new();
        object.add_property_str("userId", Some(&sender.id.to_string()));
        object.add_property_str("userDisplayName", Some(&sender.name));
        object.add_property_str("server", Some(&server_id));
        object.add_property_str("room", Some(&room_id));
        object.add_property_str("area", Some("JavaChatRealms"));
        object.add_property_str("data", Some(message));
        object.add_property_str("language", Some("*"));
        object
    });
    let strategy = IgnoreStrategy::select(hashes_to_drop);
    let pool = ServerTextFilter::create_worker_pool(max_concurrent_requests)?;

    let Some(certificate) = load_certificate(&certificate_path, &certificate_password) else {
        log::warn!("Failed to open certificate file");
        return Ok(None);
    };
    let authority_host =
        std::env::var(AUTHORITY_HOST_ENV).unwrap_or_else(|_| DEFAULT_AUTHORITY_HOST.to_owned());
    let Some(authority) = build_authority(&authority_host, &tenant_id) else {
        log::warn!("Failed to create confidential client application");
        return Ok(None);
    };
    let backend = PlayerSafetyServiceTextFilter {
        client: ConfidentialClient::new(authority, application_id, scope, certificate),
        fully_filtered_events,
        connection_read_timeout_ms,
    };
    Ok(Some(ServerTextFilter::new(
        chat_endpoint,
        chat_encoder,
        strategy,
        pool,
        Box::new(backend),
    )))
}

/// `Authority.createAuthority`: `https://login.microsoftonline.com/<tenantId>/` must be a URL with at least one path
/// segment and no fragment.
fn build_authority(host: &str, tenant_id: &str) -> Option<reqwest::Url> {
    let authority =
        reqwest::Url::parse(&format!("{}/{tenant_id}/", host.trim_end_matches('/'))).ok()?;
    let has_segment = !authority.path().trim_matches('/').is_empty();
    (has_segment && authority.fragment().is_none()).then_some(authority)
}

/// `ClientCredentialFactory.createFromCertificate(InputStream, password)`: the first private key entry of the PKCS12
/// file; `None` for whatever makes it throw (unreadable, not PKCS12, wrong password, no RSA key or certificate).
fn load_certificate(path: &str, password: &str) -> Option<ClientCertificate> {
    let bytes = std::fs::read(path).ok()?;
    let store = p12_keystore::KeyStore::from_pkcs12(
        &bytes,
        password,
        p12_keystore::Pkcs12ImportPolicy::Strict,
    )
    .ok()?;
    let (_, chain) = store.private_key_chain()?;
    let certificates: Vec<&[u8]> = chain.certs().iter().map(|c| c.as_der()).collect();
    let key = RsaPrivateKey::from_pkcs8_der(chain.key().as_der()).ok()?;
    ClientCertificate::new(key, &certificates)
}

/// `ClientCertificate`: the signing key with the certificate chain (leaf first) the assertion carries.
struct ClientCertificate {
    key: SigningKey<Sha256>,
    /// `x5t#S256`: the base64url SHA-256 of the leaf certificate.
    thumbprint: String,
    /// `x5c`: the base64 DER chain.
    chain: Vec<String>,
}

impl ClientCertificate {
    fn new(key: RsaPrivateKey, chain: &[&[u8]]) -> Option<Self> {
        let leaf = chain.first()?;
        Some(Self {
            key: SigningKey::new(key),
            thumbprint: URL_SAFE_NO_PAD.encode(Sha256::digest(leaf)),
            chain: chain.iter().map(|der| STANDARD.encode(der)).collect(),
        })
    }

    /// `JwtHelper.buildJwt(clientId, credential, jwtAudience, sendX5c = true, useSha1 = false)`.
    fn client_assertion(&self, audience: &str, client_id: &str, now: u64, jti: &str) -> String {
        let header = json!({
            "alg": "RS256",
            "typ": "JWT",
            "x5c": self.chain,
            "x5t#S256": self.thumbprint,
        });
        let claims = json!({
            "aud": audience,
            "iss": client_id,
            "sub": client_id,
            "nbf": now,
            "exp": now + ASSERTION_LIFETIME_SECONDS,
            "jti": jti,
        });
        let input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let signature = self.key.sign(input.as_bytes());
        format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }
}

struct AuthState {
    token_endpoint: Option<String>,
    /// The access token and the moment it is renewed.
    token: Option<(String, Instant)>,
}

/// `ConfidentialClientApplication` of one tenant, for the one scope `ClientCredentialParameters` holds.
struct ConfidentialClient {
    http: reqwest::blocking::Client,
    authority: reqwest::Url,
    application_id: String,
    scope: String,
    certificate: ClientCertificate,
    state: Mutex<AuthState>,
}

impl ConfidentialClient {
    fn new(
        authority: reqwest::Url,
        application_id: String,
        scope: String,
        certificate: ClientCertificate,
    ) -> Self {
        // MSAL has its own timeouts, not the filter's read timeout
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(20))
            .no_proxy()
            .build()
            .expect("an HTTP client");
        Self {
            http,
            authority,
            application_id,
            scope,
            certificate,
            state: Mutex::new(AuthState {
                token_endpoint: None,
                token: None,
            }),
        }
    }

    /// `acquireToken(parameters).join()`: the cached token, else a fresh one. Failures are the `CompletionException`
    /// `join()` throws.
    fn access_token(&self) -> Result<String, FilterError> {
        let mut state = self.state.lock();
        if let Some((token, renew_at)) = &state.token
            && Instant::now() < *renew_at
        {
            return Ok(token.clone());
        }
        let (token, lifetime) = self.request_token(&mut state).map_err(|cause| {
            JavaException::unchecked(
                "java.util.concurrent.CompletionException",
                cause.to_string(),
            )
        })?;
        state.token = Some((
            token.clone(),
            Instant::now() + lifetime.saturating_sub(REFRESH_MARGIN),
        ));
        Ok(token)
    }

    fn request_token(&self, state: &mut AuthState) -> Result<(String, Duration), FilterError> {
        let endpoint = state
            .token_endpoint
            .get_or_insert_with(|| self.discover_token_endpoint())
            .clone();
        let assertion = self.certificate.client_assertion(
            &endpoint,
            &self.application_id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            &uuid::Uuid::new_v4().to_string(),
        );
        let body = form(&[
            ("scope", &self.scope),
            ("client_id", &self.application_id),
            ("grant_type", "client_credentials"),
            ("client_assertion_type", JWT_BEARER),
            ("client_assertion", &assertion),
            ("client_info", "1"),
        ]);
        let response = self
            .http
            .post(&endpoint)
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header("Accept", "application/json")
            .header("client-request-id", uuid::Uuid::new_v4().to_string())
            .header("return-client-request-id", "true")
            .header("x-client-SKU", "MSAL.Java")
            .header("x-client-VER", "1.24.1")
            .body(body)
            .send()
            .map_err(transport_error)?;
        let text = read_body(response)?;
        let answer: Value = serde_json::from_str(&text)
            .map_err(|e| JavaException::unchecked(MSAL_SERVICE_EXCEPTION, e.to_string()))?;
        let token = answer
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| JavaException::unchecked(MSAL_SERVICE_EXCEPTION, text.clone()))?;
        let expires_in = match answer.get("expires_in") {
            Some(Value::Number(n)) => n.as_u64(),
            Some(Value::String(s)) => s.parse().ok(),
            _ => None,
        }
        .unwrap_or(0);
        Ok((token.to_owned(), Duration::from_secs(expires_in)))
    }

    /// The instance discovery and OpenID metadata requests of a first token request. MSAL falls back to the
    /// authority's own endpoints when they fail, so does this.
    fn discover_token_endpoint(&self) -> String {
        let default = format!("{}oauth2/v2.0/token", self.authority);
        let origin = self.authority.origin().ascii_serialization();
        let authorize = format!("{}oauth2/v2.0/authorize", self.authority);
        if let Ok(mut url) = reqwest::Url::parse(&format!("{origin}/common/discovery/instance")) {
            url.query_pairs_mut()
                .append_pair("api-version", "1.1")
                .append_pair("authorization_endpoint", &authorize);
            if let Err(e) = self.get(url) {
                log::debug!("Instance discovery failed: {e}");
            }
        }
        let metadata = format!("{}v2.0/.well-known/openid-configuration", self.authority);
        match reqwest::Url::parse(&metadata)
            .map_err(|e| JavaException::unchecked(MSAL_CLIENT_EXCEPTION, e.to_string()))
            .and_then(|url| self.get(url))
            .and_then(|text| {
                serde_json::from_str::<Value>(&text)
                    .map_err(|e| JavaException::unchecked(MSAL_SERVICE_EXCEPTION, e.to_string()))
            }) {
            Ok(answer) => answer
                .get("token_endpoint")
                .and_then(Value::as_str)
                .map_or(default, str::to_owned),
            Err(e) => {
                log::debug!("OpenID metadata request failed: {e}");
                default
            }
        }
    }

    fn get(&self, url: reqwest::Url) -> Result<String, FilterError> {
        let response = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .send()
            .map_err(transport_error)?;
        read_body(response)
    }
}

/// `application/x-www-form-urlencoded` of `pairs`.
fn form(pairs: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse("http://form.invalid/").expect("a URL");
    url.query_pairs_mut().extend_pairs(pairs);
    url.query().unwrap_or_default().to_owned()
}

/// The body of a 2xx answer; any other status is the `MsalServiceException` MSAL throws, with the description of
/// the error answer (its whole body when it has none).
fn read_body(response: reqwest::blocking::Response) -> Result<String, FilterError> {
    let status = response.status();
    let text = response.text().map_err(transport_error)?;
    if status.is_success() {
        return Ok(text);
    }
    let message = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.get("error_description")?.as_str().map(str::to_owned))
        .unwrap_or(text);
    Err(JavaException::unchecked(MSAL_SERVICE_EXCEPTION, message))
}

/// `MsalClientException(IOException)`: its message is the `toString()` of the IO exception.
fn transport_error(e: reqwest::Error) -> FilterError {
    let cause = if e.is_timeout() {
        let message = if e.is_connect() {
            "Connect timed out"
        } else {
            "Read timed out"
        };
        JavaException::new("java.net.SocketTimeoutException", message)
    } else if e.is_connect() {
        JavaException::new("java.net.ConnectException", "Connection refused")
    } else {
        JavaException::new("java.io.IOException", e.to_string())
    };
    JavaException::unchecked(MSAL_CLIENT_EXCEPTION, cause.to_string())
}

impl FilterBackend for PlayerSafetyServiceTextFilter {
    fn authorization(&self) -> Result<String, FilterError> {
        Ok(format!("Bearer {}", self.client.access_token()?))
    }

    fn filter_text(
        &self,
        message: &str,
        strategy: IgnoreStrategy,
        response: &JsonObject,
    ) -> Result<FilteredText, FilterError> {
        let Some(result) = get_as_object_opt(response, "result")? else {
            return Ok(FilteredText::fully_filtered(message));
        };
        if !get_as_bool_or(result, "filtered", true)? {
            return Ok(FilteredText::pass_through(message));
        }
        for element in get_as_array_or_empty(result, "events")? {
            let event = get_as_string_or(as_object(element)?, "id", "")?;
            if self.fully_filtered_events.contains(&event) {
                return Ok(FilteredText::fully_filtered(message));
            }
        }
        let redacted = get_as_array_or_empty(result, "redactedTextIndex")?;
        Ok(FilteredText {
            raw: message.to_owned(),
            mask: parse_mask(message, redacted, strategy)?,
        })
    }

    fn read_timeout(&self) -> Duration {
        // shortcut: Java's 0 (no timeout) and negative (IllegalArgumentException per request) become one day.
        match u64::try_from(self.connection_read_timeout_ms) {
            Ok(ms) if ms > 0 => Duration::from_millis(ms),
            _ => Duration::from_secs(24 * 60 * 60),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::gson::JsonElement;
    use mc_protocol::packets::play::FilterMask;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    fn config_with(change: impl FnOnce(&mut Value)) -> String {
        let mut config = json!({
            "apiServer": "https://safety.example",
            "apiPath": "/v1/chat",
            "scope": "api://x/.default",
            "applicationId": "app",
            "tenantId": "tenant",
            "certificatePath": "/nonexistent/cert.p12",
            "fullyFilteredEvents": ["bad"],
        });
        change(&mut config);
        config.to_string()
    }

    fn error_of(config: &str) -> String {
        match create_text_filter_from_config(config) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected an error"),
        }
    }

    #[test]
    fn config_errors_are_returned() {
        assert_eq!(
            error_of(&config_with(|c| {
                c.as_object_mut().unwrap().remove("scope");
            })),
            "com.google.gson.JsonSyntaxException: Missing scope, expected to find a string"
        );
        assert_eq!(
            error_of(&config_with(|c| {
                c.as_object_mut().unwrap().remove("fullyFilteredEvents");
            })),
            "com.google.gson.JsonSyntaxException: Missing fullyFilteredEvents, expected to find a JsonArray"
        );
        assert_eq!(
            error_of(&config_with(|c| c["fullyFilteredEvents"] = json!([[]]))),
            "com.google.gson.JsonSyntaxException: Expected filteredEvent to be a string, was an array ([])"
        );
        assert_eq!(
            error_of(&config_with(|c| c["maxConcurrentRequests"] = json!("x"))),
            "com.google.gson.JsonSyntaxException: Expected maxConcurrentRequests to be a Int, was \"x\""
        );
        assert!(error_of("[]").starts_with("com.google.gson.JsonSyntaxException"));
    }

    #[test]
    fn an_unreadable_certificate_is_no_filter() {
        assert!(matches!(
            create_text_filter_from_config(&config_with(|_| {})),
            Ok(None)
        ));
        let garbage = tempfile::NamedTempFile::new().expect("a file");
        std::fs::write(garbage.path(), b"not a pkcs12 file").expect("write");
        assert!(load_certificate(garbage.path().to_str().unwrap(), "").is_none());
    }

    #[test]
    fn authority_needs_a_tenant() {
        assert!(build_authority(DEFAULT_AUTHORITY_HOST, "tenant").is_some());
        assert!(build_authority(DEFAULT_AUTHORITY_HOST, "").is_none());
        assert!(build_authority(DEFAULT_AUTHORITY_HOST, "a#b").is_none());
        assert!(build_authority("not a url", "tenant").is_none());
    }

    fn backend() -> PlayerSafetyServiceTextFilter {
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("a key");
        let certificate = ClientCertificate::new(key, &[b"leaf"]).expect("a certificate");
        PlayerSafetyServiceTextFilter {
            client: ConfidentialClient::new(
                reqwest::Url::parse("https://login.invalid/tenant/").unwrap(),
                "app".to_owned(),
                "scope".to_owned(),
                certificate,
            ),
            fully_filtered_events: HashSet::from(["bad".to_owned()]),
            connection_read_timeout_ms: 2000,
        }
    }

    fn answer(text: &str) -> JsonObject {
        match mc_core::gson::parse_lenient_json_parser(text).expect("json") {
            JsonElement::Object(o) => o,
            _ => panic!("an object"),
        }
    }

    #[test]
    fn filter_text_rows() {
        let b = backend();
        let run = |strategy, text: &str| {
            b.filter_text("hello", strategy, &answer(text))
                .expect("a row")
        };
        let never = IgnoreStrategy::NeverIgnore;
        assert_eq!(run(never, "{}"), FilteredText::fully_filtered("hello"));
        assert_eq!(
            run(
                never,
                r#"{"result":{"filtered":false,"events":[{"id":"bad"}]}}"#
            ),
            FilteredText::pass_through("hello")
        );
        assert_eq!(
            run(never, r#"{"result":{"events":[{"id":"ok"},{"id":"bad"}]}}"#),
            FilteredText::fully_filtered("hello")
        );
        assert_eq!(
            run(never, r#"{"result":{"events":[{}]}}"#),
            FilteredText::pass_through("hello")
        );
        let partial = run(never, r#"{"result":{"redactedTextIndex":[0,1]}}"#);
        assert_eq!(partial.raw, "hello");
        assert!(matches!(partial.mask, FilterMask::PartiallyFiltered(_)));
        assert_eq!(
            run(
                IgnoreStrategy::IgnoreFullyFiltered,
                r#"{"result":{"redactedTextIndex":[0,1,2,3,4]}}"#
            ),
            FilteredText::fully_filtered("hello")
        );
        assert!(
            b.filter_text("hello", never, &answer(r#"{"result":{"events":[1]}}"#))
                .is_err()
        );
        assert_eq!(b.read_timeout(), Duration::from_millis(2000));
    }

    fn decode(part: &str) -> Value {
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).expect("base64url")).expect("json")
    }

    /// Checks the three parts of `jwt` against the certificate and returns the claims.
    fn verify_assertion(jwt: &str, key: &RsaPrivateKey, chain: &[&[u8]]) -> Value {
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header = decode(parts[0]);
        assert_eq!(header["alg"], "RS256");
        assert_eq!(header["typ"], "JWT");
        let x5c: Vec<String> = chain.iter().map(|der| STANDARD.encode(der)).collect();
        assert_eq!(header["x5c"], json!(x5c));
        assert_eq!(
            header["x5t#S256"],
            URL_SAFE_NO_PAD.encode(Sha256::digest(chain[0]))
        );
        let signature = URL_SAFE_NO_PAD.decode(parts[2]).expect("a signature");
        VerifyingKey::<Sha256>::new(key.to_public_key())
            .verify(
                format!("{}.{}", parts[0], parts[1]).as_bytes(),
                &Signature::try_from(signature.as_slice()).expect("a signature"),
            )
            .expect("a valid signature");
        decode(parts[1])
    }

    #[test]
    fn assertion_shape_and_signature() {
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("a key");
        let chain: [&[u8]; 2] = [b"leaf certificate", b"root certificate"];
        let certificate = ClientCertificate::new(key.clone(), &chain).expect("a certificate");
        let jwt = certificate.client_assertion("https://aad/token", "app", 1000, "jti-1");
        let claims = verify_assertion(&jwt, &key, &chain);
        assert_eq!(
            claims,
            json!({"aud":"https://aad/token","iss":"app","sub":"app","nbf":1000,"exp":1600,"jti":"jti-1"})
        );
    }

    /// A mock authority: answers discovery, metadata (pointing at `/tenant/token`) and the token endpoint with
    /// `token_status`/`token_body`; every request is reported as (method and path, body).
    fn serve(
        token_status: u16,
        token_body: &'static str,
    ) -> (String, mpsc::Receiver<(String, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let (sender, receiver) = mpsc::channel();
        let served = base.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.expect("accept");
                let (request, body) = read_request(&mut stream);
                let (status, reply) = if request.contains("/common/discovery/instance") {
                    (200, "{}".to_owned())
                } else if request.contains("/.well-known/openid-configuration") {
                    (
                        200,
                        format!(r#"{{"token_endpoint":"{served}/tenant/token"}}"#),
                    )
                } else {
                    (token_status, token_body.to_owned())
                };
                let _ = sender.send((request, body));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            }
        });
        (base, receiver)
    }

    fn read_request(stream: &mut TcpStream) -> (String, String) {
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        let mut needed = None;
        loop {
            if let Some((head_end, length)) = needed
                && data.len() >= head_end + length
            {
                break;
            }
            let read = stream.read(&mut buffer).expect("read");
            assert!(read > 0, "the request ended early");
            data.extend_from_slice(&buffer[..read]);
            if needed.is_none()
                && let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n")
            {
                let head = String::from_utf8_lossy(&data[..at]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .map_or(0, |v| v.trim().parse().expect("a length"));
                needed = Some((at + 4, length));
            }
        }
        let text = String::from_utf8_lossy(&data).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").expect("a head");
        (
            head.lines().next().unwrap_or_default().to_owned(),
            body.to_owned(),
        )
    }

    fn mock_client(base: &str, key: &RsaPrivateKey) -> ConfidentialClient {
        let certificate = ClientCertificate::new(key.clone(), &[b"leaf"]).expect("a certificate");
        ConfidentialClient::new(
            reqwest::Url::parse(&format!("{base}/tenant/")).unwrap(),
            "app".to_owned(),
            "api://x/.default".to_owned(),
            certificate,
        )
    }

    #[test]
    fn token_flow_and_cache() {
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("a key");
        let (base, requests) = serve(
            200,
            r#"{"token_type":"Bearer","expires_in":3600,"access_token":"tok"}"#,
        );
        let client = mock_client(&base, &key);
        assert_eq!(client.access_token().expect("a token"), "tok");
        assert_eq!(client.access_token().expect("the cached token"), "tok");
        let seen: Vec<(String, String)> = requests.try_iter().collect();
        assert_eq!(
            seen.len(),
            3,
            "discovery, metadata and one token request: {seen:?}"
        );
        assert!(
            seen[0].0.starts_with(
                "GET /common/discovery/instance?api-version=1.1&authorization_endpoint="
            )
        );
        assert!(seen[0].0.contains(&format!(
            "authorization_endpoint={}",
            form(&[("", &format!("{base}/tenant/oauth2/v2.0/authorize"))])[1..].to_owned()
        )));
        assert!(
            seen[1]
                .0
                .starts_with("GET /tenant/v2.0/.well-known/openid-configuration ")
        );
        assert!(seen[2].0.starts_with("POST /tenant/token "));
        let params: Vec<(&str, &str)> = seen[2]
            .1
            .split('&')
            .map(|p| p.split_once('=').expect("a parameter"))
            .collect();
        let get = |name: &str| params.iter().find(|(k, _)| *k == name).expect(name).1;
        assert_eq!(get("scope"), "api%3A%2F%2Fx%2F.default");
        assert_eq!(get("client_id"), "app");
        assert_eq!(get("grant_type"), "client_credentials");
        assert_eq!(
            get("client_assertion_type"),
            "urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
        );
        let claims = verify_assertion(get("client_assertion"), &key, &[b"leaf"]);
        assert_eq!(claims["aud"], format!("{base}/tenant/token"));
        assert_eq!(claims["iss"], "app");
        assert_eq!(claims["sub"], "app");
        assert_eq!(
            claims["exp"].as_u64().unwrap() - claims["nbf"].as_u64().unwrap(),
            600
        );
    }

    #[test]
    fn a_short_lived_token_is_requested_again() {
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("a key");
        let (base, requests) = serve(200, r#"{"expires_in":299,"access_token":"tok"}"#);
        let client = mock_client(&base, &key);
        client.access_token().expect("a token");
        client.access_token().expect("a token");
        // discovery and metadata once, the token twice
        assert_eq!(requests.try_iter().count(), 4);
    }

    #[test]
    fn a_service_error_is_a_completion_exception() {
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("a key");
        let (base, _requests) = serve(
            401,
            r#"{"error":"invalid_client","error_description":"AADSTS7000215: bad secret"}"#,
        );
        let e = mock_client(&base, &key)
            .access_token()
            .expect_err("an error");
        assert_eq!(
            e.to_string(),
            "java.util.concurrent.CompletionException: com.microsoft.aad.msal4j.MsalServiceException: AADSTS7000215: bad secret"
        );
    }

    #[test]
    fn an_unreachable_authority_is_a_client_exception() {
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).expect("a key");
        let free = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", free.local_addr().unwrap());
        drop(free);
        let e = mock_client(&base, &key)
            .access_token()
            .expect_err("an error");
        assert_eq!(
            e.to_string(),
            "java.util.concurrent.CompletionException: com.microsoft.aad.msal4j.MsalClientException: java.net.ConnectException: Connection refused"
        );
    }
}
