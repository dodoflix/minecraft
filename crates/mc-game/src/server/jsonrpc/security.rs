//! `net.minecraft.server.jsonrpc.security`: the shared secret, the HTTP-upgrade authentication rules and the TLS
//! context built from a PKCS12 keystore.

use std::collections::HashSet;
use std::fs;
use std::sync::Arc;

use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

const SUBPROTOCOL_VALUE: &str = "minecraft-v1";
const SUBPROTOCOL_HEADER_PREFIX: &str = "minecraft-v1,";
const BEARER_PREFIX: &str = "Bearer ";
const PASSWORD_ENV_VARIABLE_KEY: &str = "MINECRAFT_MANAGEMENT_TLS_KEYSTORE_PASSWORD";

/// `SecurityConfig.isValid`: exactly 40 ASCII letters or digits.
pub fn is_valid_secret(secret_key: &str) -> bool {
    secret_key.len() == 40 && secret_key.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The outcome of `AuthenticationHandler.performSecurityChecks`.
#[derive(Debug, PartialEq, Eq)]
pub enum SecurityCheckResult {
    /// `token_in_ws_protocol`: the key came in `Sec-WebSocket-Protocol`, so the 101 response must answer with it.
    Allowed { token_in_ws_protocol: bool },
    Denied(&'static str),
}

/// `AuthenticationHandler`.
pub struct AuthenticationHandler {
    secret_key: String,
    allowed_origins: HashSet<String>,
}

impl AuthenticationHandler {
    pub fn new(secret_key: String, allowed_origins: &str) -> AuthenticationHandler {
        AuthenticationHandler { secret_key, allowed_origins: allowed_origins.split(',').map(str::to_owned).collect() }
    }

    /// `isValidApiKey`: constant-time comparison of the UTF-8 bytes (`MessageDigest.isEqual`).
    pub fn is_valid_api_key(&self, supplied: &str) -> bool {
        if supplied.is_empty() {
            return false;
        }
        let (a, b) = (supplied.as_bytes(), self.secret_key.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }

    /// `performSecurityChecks` on the three relevant request headers.
    pub fn check(&self, authorization: Option<&str>, sec_websocket_protocol: Option<&str>, origin: Option<&str>) -> SecurityCheckResult {
        if let Some(token) = authorization.and_then(|h| h.strip_prefix(BEARER_PREFIX)).map(str::trim) {
            return if self.is_valid_api_key(token) {
                SecurityCheckResult::Allowed { token_in_ws_protocol: false }
            } else {
                SecurityCheckResult::Denied("Invalid API key")
            };
        }
        if let Some(token) = sec_websocket_protocol.and_then(|h| h.strip_prefix(SUBPROTOCOL_HEADER_PREFIX)).map(str::trim) {
            if !origin.is_some_and(|o| !o.is_empty() && self.allowed_origins.contains(o)) {
                return SecurityCheckResult::Denied("Origin Not Allowed");
            }
            return if self.is_valid_api_key(token) {
                SecurityCheckResult::Allowed { token_in_ws_protocol: true }
            } else {
                SecurityCheckResult::Denied("Invalid API key")
            };
        }
        SecurityCheckResult::Denied("Missing API key")
    }

    /// The `Sec-WebSocket-Protocol` value of the 101 response when the key was sent that way.
    pub fn subprotocol_value() -> &'static str {
        SUBPROTOCOL_VALUE
    }
}

/// The body of the 401 reply (`sendUnauthorizedResponse`).
pub fn unauthorized_body(reason: &str) -> String {
    format!("{{\"error\":\"Unauthorized\",\"message\":\"{reason}\"}}")
}

/// `JsonRpcSslContextProvider.printInstructions`.
pub fn print_tls_instructions() {
    log::info!("To use TLS for the management server, please follow these steps:");
    log::info!("1. Set the server property 'management-server-tls-enabled' to 'true' to enable TLS");
    log::info!("2. Create a keystore file of type PKCS12 containing your server certificate and private key");
    log::info!("3. Set the server property 'management-server-tls-keystore' to the path of your keystore file");
    log::info!(
        "4. Set the keystore password via the environment variable 'MINECRAFT_MANAGEMENT_TLS_KEYSTORE_PASSWORD', or system property 'management.tls.keystore.password', or server property 'management-server-tls-keystore-password'"
    );
    log::info!("5. Restart the server to apply the changes.");
}

/// `JsonRpcSslContextProvider.createFrom`. `Err` carries the Java exception text.
pub fn create_tls_acceptor(keystore_path: &str, password_from_properties: &str) -> Result<TlsAcceptor, String> {
    if keystore_path.is_empty() {
        return Err("java.lang.IllegalArgumentException: TLS is enabled but keystore is not configured".to_owned());
    }
    let file = std::path::Path::new(keystore_path);
    if !file.is_file() {
        return Err(format!("java.lang.IllegalArgumentException: Supplied keystore is not a file or does not exist: '{keystore_path}'"));
    }
    let password = std::env::var(PASSWORD_ENV_VARIABLE_KEY).unwrap_or_else(|_| password_from_properties.to_owned());
    let bytes = fs::read(file).map_err(|e| format!("java.io.IOException: {e}"))?;
    let keystore = p12_keystore::KeyStore::from_pkcs12(&bytes, &password).map_err(|e| format!("java.io.IOException: {e}"))?;
    let (_, chain) = keystore
        .private_key_chain()
        .ok_or_else(|| "java.security.UnrecoverableKeyException: no private key entry in the keystore".to_owned())?;
    let certs: Vec<CertificateDer<'static>> = chain.chain().iter().map(|c| CertificateDer::from(c.as_der().to_vec())).collect();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(chain.key().to_vec()));
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("javax.net.ssl.SSLException: {e}"))?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";

    #[test]
    fn secret_validation() {
        assert!(is_valid_secret(KEY));
        assert!(!is_valid_secret(""));
        assert!(!is_valid_secret(&KEY[1..]));
        assert!(!is_valid_secret("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklm_"));
    }

    #[test]
    fn auth_rules() {
        let h = AuthenticationHandler::new(KEY.to_owned(), "https://a.example,https://b.example");
        let ok = SecurityCheckResult::Allowed { token_in_ws_protocol: false };
        assert_eq!(h.check(Some(&format!("Bearer {KEY}")), None, None), ok);
        assert_eq!(h.check(Some(&format!("Bearer  {KEY} ")), None, None), ok);
        assert_eq!(h.check(Some("Bearer wrong"), None, None), SecurityCheckResult::Denied("Invalid API key"));
        assert_eq!(h.check(Some("Basic abc"), None, None), SecurityCheckResult::Denied("Missing API key"));
        assert_eq!(h.check(None, None, None), SecurityCheckResult::Denied("Missing API key"));
        let proto = format!("minecraft-v1,{KEY}");
        assert_eq!(h.check(None, Some(&proto), None), SecurityCheckResult::Denied("Origin Not Allowed"));
        assert_eq!(h.check(None, Some(&proto), Some("https://evil.example")), SecurityCheckResult::Denied("Origin Not Allowed"));
        assert_eq!(h.check(None, Some(&proto), Some("https://b.example")), SecurityCheckResult::Allowed { token_in_ws_protocol: true });
        // an empty allowed-origins list never admits the empty origin
        let none = AuthenticationHandler::new(KEY.to_owned(), "");
        assert_eq!(none.check(None, Some(&proto), Some("")), SecurityCheckResult::Denied("Origin Not Allowed"));
        // a Bearer header takes precedence over the sub-protocol token
        assert_eq!(h.check(Some("Bearer nope"), Some(&proto), Some("https://a.example")), SecurityCheckResult::Denied("Invalid API key"));
    }

    #[test]
    fn unauthorized_json() {
        assert_eq!(unauthorized_body("Missing API key"), "{\"error\":\"Unauthorized\",\"message\":\"Missing API key\"}");
    }
}
