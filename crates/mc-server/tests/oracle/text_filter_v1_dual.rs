//! The text filter (version 1: an Azure AD application whose certificate authenticates to the Player Safety service)
//! on the real vanilla server and on `mc-server`.
//!
//! The vanilla JVM talks to `https://login.microsoftonline.com` through a CONNECT proxy of this test that terminates
//! TLS with a leaf certificate of a test CA (`keytool` puts the CA into the truststore of the JVM), and forwards the
//! decrypted requests to the mock. `mc-server` is pointed at the mock directly. Both sides send the instance
//! discovery, the OpenID metadata and the token request there, then the chat requests with the bearer token; the
//! rows compare those requests (the signed client assertion decoded) and what the players see and the server logs.
use super::text_filter_dual::{chat_packets, connect, filter_log};
use mc_testbot::harness;
use mc_testkit::{FilterAnswer, FilterRequest, MockFilterServer, Server};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

const AUTHORITY: &str = "https://login.microsoftonline.com";
const TENANT: &str = "tenant-1";
const APPLICATION: &str = "app-1";
const P12_PASSWORD: &str = "secret";

/// Runs a tool of the host (`openssl`, `keytool`) and fails with its output.
fn run(program: &str, args: &[&str], dir: &Path) {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("{program}: {e}"));
    assert!(
        out.status.success(),
        "{program} {args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The certificates of a run, all in one temporary directory.
struct Pki {
    dir: tempfile::TempDir,
}

impl Pki {
    /// A CA, a leaf for `login.microsoftonline.com` signed by it, a truststore holding the CA and the PKCS12 file with
    /// the key and certificate of the application.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let d = dir.path();
        run(
            "openssl",
            &[
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                "ca.key",
                "-out",
                "ca.pem",
                "-subj",
                "/CN=Test CA",
                "-days",
                "3",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
                "-addext",
                "keyUsage=critical,keyCertSign,cRLSign",
            ],
            d,
        );
        run(
            "openssl",
            &[
                "req",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                "leaf.key",
                "-out",
                "leaf.csr",
                "-subj",
                "/CN=login.microsoftonline.com",
            ],
            d,
        );
        std::fs::write(
            d.join("leaf.ext"),
            "subjectAltName=DNS:login.microsoftonline.com\nbasicConstraints=CA:FALSE\n",
        )
        .expect("extension file");
        run(
            "openssl",
            &[
                "x509",
                "-req",
                "-in",
                "leaf.csr",
                "-CA",
                "ca.pem",
                "-CAkey",
                "ca.key",
                "-CAcreateserial",
                "-out",
                "leaf.pem",
                "-days",
                "3",
                "-extfile",
                "leaf.ext",
            ],
            d,
        );
        run(
            "keytool",
            &[
                "-importcert",
                "-noprompt",
                "-alias",
                "ca",
                "-file",
                "ca.pem",
                "-keystore",
                "truststore.p12",
                "-storetype",
                "PKCS12",
                "-storepass",
                "changeit",
            ],
            d,
        );
        run(
            "openssl",
            &[
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                "app.key",
                "-out",
                "app.pem",
                "-subj",
                "/CN=filter-application",
                "-days",
                "3",
            ],
            d,
        );
        run(
            "openssl",
            &[
                "pkcs12",
                "-export",
                "-inkey",
                "app.key",
                "-in",
                "app.pem",
                "-name",
                "app",
                "-out",
                "app.p12",
                "-passout",
                &format!("pass:{P12_PASSWORD}"),
            ],
            d,
        );
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn tls(&self) -> TlsAcceptor {
        let chain = CertificateDer::pem_file_iter(self.path("leaf.pem"))
            .expect("leaf certificate")
            .collect::<Result<Vec<_>, _>>()
            .expect("leaf certificate");
        let key = PrivateKeyDer::from_pem_file(self.path("leaf.key")).expect("leaf key");
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .expect("server config");
        TlsAcceptor::from(Arc::new(config))
    }
}

/// A CONNECT proxy: tunnels to `mock_port` whatever target is asked for, behind TLS made with `tls`. Counts the tunnels.
async fn start_proxy(tls: TlsAcceptor, mock_port: u16) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("proxy port");
    let port = listener.local_addr().expect("proxy address").port();
    let tunnels = Arc::new(AtomicUsize::new(0));
    let counted = tunnels.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut client, _)) = listener.accept().await else {
                return;
            };
            let (tls, counted) = (tls.clone(), counted.clone());
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if client.read(&mut byte).await.ok()? == 0 {
                        return None;
                    }
                    head.push(byte[0]);
                }
                assert!(head.starts_with(b"CONNECT login.microsoftonline.com:443 "));
                counted.fetch_add(1, Ordering::SeqCst);
                client
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await
                    .ok()?;
                let mut client = tls.accept(client).await.ok()?;
                let mut mock = tokio::net::TcpStream::connect(("127.0.0.1", mock_port))
                    .await
                    .ok()?;
                tokio::io::copy_bidirectional(&mut client, &mut mock)
                    .await
                    .ok()
            });
        }
    });
    (port, tunnels)
}

/// What the authority and the Player Safety service answer.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Tokens live an hour.
    Plain,
    /// Tokens expire within the five minutes MSAL renews them before.
    ShortTokens,
    /// The token endpoint refuses the client.
    TokenRefused,
    /// Instance discovery fails: MSAL falls back to the metadata it knows.
    NoDiscovery,
    /// Instance discovery answers with [`discovery_answer`] number `n`.
    DiscoveryAnswer(u8),
}

/// The answers of the authority and the Player Safety service.
fn answer(mode: Mode, tokens: Arc<AtomicUsize>) -> impl Fn(&FilterRequest) -> FilterAnswer {
    move |r| {
        let path = r.target.split('?').next().unwrap_or("");
        if path.ends_with("/discovery/instance") {
            if mode == Mode::NoDiscovery {
                return FilterAnswer::Status(500);
            }
            if let Mode::DiscoveryAnswer(n) = mode {
                let (status, body) = discovery_answer(n);
                return FilterAnswer::Json(status, body);
            }
            return FilterAnswer::Json(
                200,
                format!(
                    r#"{{"tenant_discovery_endpoint":"{AUTHORITY}/{TENANT}/v2.0/.well-known/openid-configuration","api-version":"1.1","metadata":[{{"preferred_network":"login.microsoftonline.com","preferred_cache":"login.windows.net","aliases":["login.microsoftonline.com","login.windows.net"]}}]}}"#
                ),
            );
        }
        if path.ends_with("/oauth2/v2.0/token") {
            if mode == Mode::TokenRefused {
                return FilterAnswer::Json(
                    401,
                    r#"{"error":"invalid_client","error_description":"AADSTS700027: refused"}"#
                        .into(),
                );
            }
            let n = tokens.fetch_add(1, Ordering::SeqCst) + 1;
            let expires = if mode == Mode::ShortTokens { 120 } else { 3600 };
            return FilterAnswer::Json(
                200,
                format!(
                    r#"{{"token_type":"Bearer","expires_in":{expires},"ext_expires_in":{expires},"access_token":"token-{n}"}}"#
                ),
            );
        }
        let text = r.body.split(r#""data":""#).nth(1).unwrap_or("");
        let text = text.split('"').next().unwrap_or("");
        let result = |s: &str| FilterAnswer::Json(200, format!(r#"{{"result":{s}}}"#));
        match text {
            "pass" => result(r#"{"filtered":false}"#),
            "hashed" => result(r#"{"filtered":true,"redactedTextIndex":[1,2]}"#),
            "badword" => result(
                r#"{"filtered":true,"events":[{"id":"x"},{"id":"bad"}],"redactedTextIndex":[0]}"#,
            ),
            "noresult" => FilterAnswer::Json(200, "{}".into()),
            "allhashed" => result(r#"{"filtered":true,"redactedTextIndex":[0,1,2,3,4,5,6,7,8]}"#),
            _ => result(r#"{"filtered":false}"#),
        }
    }
}

/// Answers of the discovery endpoint that MSAL cannot use: error pages that are not JSON (markup, plain words after
/// blank space, markup over the length the parser quotes), a page with a success status, and JSON without the
/// metadata endpoint.
fn discovery_answer(n: u8) -> (u16, String) {
    match n {
        0 => (500, "<html>gateway down</html>".to_owned()),
        1 => (500, "  \n Service Unavailable, try later".to_owned()),
        2 => (500, format!("<p>{}</p>", "x".repeat(600))),
        3 => (200, "<html>ok</html>".to_owned()),
        4 => (200, r#"{"error_description":"no endpoint"}"#.to_owned()),
        5 => (200, "{}".to_owned()),
        // a service error with a description: the first token request fails, the next skips discovery
        _ => (
            500,
            r#"{"error":"server_error","error_description":"overloaded"}"#.to_owned(),
        ),
    }
}

fn percent_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn b64url(s: &str) -> Value {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .expect("base64url part");
    serde_json::from_slice(&bytes).expect("JSON part")
}

/// The client assertion with its random and clock-bound parts masked.
fn assertion(jwt: &str, origin: &str) -> String {
    let mut parts = jwt.split('.');
    let (header, claims, signature) = (parts.next(), parts.next(), parts.next());
    let (Some(header), Some(claims), Some(signature)) = (header, claims, signature) else {
        return format!("malformed assertion {jwt}");
    };
    let header = b64url(header);
    let mut claims: BTreeMap<String, Value> =
        serde_json::from_value(b64url(claims)).expect("claims");
    let lifetime =
        claims["exp"].as_u64().unwrap_or(0) as i64 - claims["nbf"].as_u64().unwrap_or(0) as i64;
    claims.insert("exp".into(), format!("nbf+{lifetime}").into());
    claims.insert("nbf".into(), "<now>".into());
    claims.insert("jti".into(), "<uuid>".into());
    let claims = serde_json::to_string(&claims)
        .expect("claims")
        .replace(origin, "<authority>");
    format!(
        "header={header} claims={claims} signature={} bytes",
        signature.len()
    )
}

/// A request as comparable text: the authority's origin masked, the request ids numbered by first appearance (two
/// requests share an id or not as they do on the other side), the form body decoded.
fn render(r: &FilterRequest, origin: &str, ids: &[String]) -> String {
    let mask = |s: &str| {
        let mut s = s.replace(origin, "<authority>");
        for (i, id) in ids.iter().enumerate() {
            s = s.replace(id, &format!("<id{i}>"));
        }
        mask_uuids(&s)
    };
    // the discovery request names the authority in its query
    let mut rows = vec![format!("{} {}", r.method, mask(&percent_decode(&r.target)))];
    // sorted: the order the JDK and reqwest send headers in is not part of the protocol
    let mut headers = Vec::new();
    for (name, value) in &r.headers {
        let shown = match name.as_str() {
            // the JDK's version, the host and the length follow from the transport; the ids and platform are per run
            "user-agent" | "host" | "content-length" | "connection" => continue,
            "x-client-os" | "x-client-cpu" => "<masked>".to_owned(),
            _ => mask(value),
        };
        headers.push(format!("{name}: {shown}"));
    }
    headers.sort();
    rows.extend(headers);
    if r.header("content-type")
        .is_some_and(|t| t.starts_with("application/x-www-form-urlencoded"))
    {
        for pair in r.body.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let (k, v) = (percent_decode(k), percent_decode(v));
            rows.push(if k == "client_assertion" {
                format!("{k}={}", assertion(&v, origin))
            } else {
                format!("{k}={}", mask(&v))
            });
        }
    } else {
        rows.push(r.body.clone());
    }
    rows.join("\n")
}

fn config(chat: &str, pki: &Pki, certificate: &str) -> String {
    format!(
        r#"{{"apiServer":"{chat}","apiPath":"/v1/chat","scope":"api://filter/.default","serverId":"srv","applicationId":"{APPLICATION}","tenantId":"{TENANT}","certificatePath":"{}","certificatePassword":"{P12_PASSWORD}","fullyFilteredEvents":["bad"],"hashesToDrop":-1}}"#,
        pki.path(certificate).display()
    )
}

/// Random ids no request carried (the id of a run that failed before its token request).
fn mask_uuids(row: &str) -> String {
    let is_uuid = |w: &str| {
        w.len() == 36
            && w.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            })
    };
    let mut out = String::new();
    let mut rest = row;
    while let Some(c) = rest.chars().next() {
        if rest.get(..36).is_some_and(is_uuid) {
            out.push_str("<uuid>");
            rest = &rest[36..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

async fn session(
    server: &Server,
    mock: &MockFilterServer,
    origin: &str,
    messages: &[&str],
) -> Vec<String> {
    let sender = connect(server.port, "Sender", false).await;
    let on = connect(server.port, "ObsOn", true).await;
    let off = connect(server.port, "ObsOff", false).await;
    for message in messages {
        sender.chat(message);
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
    let mut out = vec!["-- sender hears".to_owned()];
    out.extend(chat_packets(&sender));
    out.push("-- observer with the filter on".to_owned());
    out.extend(chat_packets(&on));
    out.push("-- observer with the filter off".to_owned());
    out.extend(chat_packets(&off));
    sender.disconnect().await;
    on.disconnect().await;
    off.disconnect().await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    // MSAL logs and reports the id of its requests; the pool thread that runs a token request is whichever is idle
    let mut ids: Vec<String> = Vec::new();
    for id in mock
        .requests()
        .iter()
        .filter_map(|r| r.header("client-request-id").map(str::to_owned))
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    out.push("-- requests".to_owned());
    out.extend(mock.requests().iter().map(|r| render(r, origin, &ids)));
    out.push("-- log".to_owned());
    out.extend(filter_log(server).into_iter().map(|mut row| {
        for (i, id) in ids.iter().enumerate() {
            row = row.replace(id, &format!("<id{i}>"));
        }
        row = mask_uuids(&row);
        // the authority host of the test seam
        let row = row.replace("for 127.0.0.1.", "for login.microsoftonline.com.");
        match row.split_once("Chat-Filter-Worker-") {
            Some((head, rest)) => format!(
                "{head}Chat-Filter-Worker-N{}",
                rest.trim_start_matches(|c: char| c.is_ascii_digit())
            ),
            None => row,
        }
    }));
    out
}

async fn run_v1(mode: Mode, messages: &[&str]) {
    let pki = Pki::new();
    let mut sides = Vec::new();
    for ours in [false, true] {
        let mock = MockFilterServer::start().expect("mock filter server");
        let tokens = Arc::new(AtomicUsize::new(0));
        let origin = if ours {
            mock.url()
        } else {
            AUTHORITY.to_owned()
        };
        mock.respond_with(answer(mode, tokens));
        let conf = config(&mock.url(), &pki, "app.p12");
        let (a, b) = (
            format!("text-filtering-config={conf}"),
            "text-filtering-version=1".to_owned(),
        );
        let props = [a.as_str(), b.as_str()];
        let mut proxy = None;
        let builder = if ours {
            crate::common::ours(7, "survival", &props)
                .env("MC_TEXT_FILTER_AUTHORITY_HOST", &mock.url())
        } else {
            let (port, tunnels) = start_proxy(pki.tls(), mock.port()).await;
            proxy = Some(tunnels);
            let store = pki.path("truststore.p12");
            harness::vanilla(7, "survival", &props)
                .java_property("javax.net.ssl.trustStore", &store.to_string_lossy())
                .java_property("javax.net.ssl.trustStorePassword", "changeit")
                .java_property("javax.net.ssl.trustStoreType", "PKCS12")
                .java_property("https.proxyHost", "127.0.0.1")
                .java_property("https.proxyPort", &port.to_string())
        };
        let server = builder.start().expect("start server");
        let rows = session(&server, &mock, &origin, messages).await;
        server.stop();
        if let Some(tunnels) = proxy {
            assert!(
                tunnels.load(Ordering::SeqCst) > 0,
                "vanilla did not use the proxy"
            );
        }
        sides.push(rows);
    }
    let [vanilla, ours] = &sides[..] else {
        unreachable!()
    };
    println!("{}", vanilla.join("\n"));
    let vanilla = vanilla.join("\n");
    let ours = ours.join("\n");
    for (i, (v, o)) in vanilla.lines().zip(ours.lines()).enumerate() {
        assert_eq!(v, o, "row {i}: vanilla (left) vs ours (right)");
    }
    assert_eq!(vanilla, ours, "vanilla (left) vs ours (right)");
}

const MESSAGES: [&str; 6] = ["pass", "hashed", "badword", "noresult", "allhashed", "pass"];

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn v1_answers_and_token_reuse() {
    run_v1(Mode::Plain, &MESSAGES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn v1_renews_tokens_that_expire_soon() {
    run_v1(Mode::ShortTokens, &["pass", "hashed", "pass"]).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn v1_token_refused() {
    run_v1(Mode::TokenRefused, &["pass", "hashed"]).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn v1_without_discovery() {
    run_v1(Mode::NoDiscovery, &["pass", "hashed"]).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn v1_discovery_answers_msal_cannot_use() {
    for n in 0..7 {
        run_v1(Mode::DiscoveryAnswer(n), &["pass", "hashed"]).await;
    }
}

/// A configuration that makes version 1 throw: the server does not start. What the log says and the exit code.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn v1_missing_fields_stop_the_server_like_vanilla() {
    let pki = Pki::new();
    let good = config("http://127.0.0.1:9", &pki, "app.p12");
    for (field, conf) in [
        ("apiPath", good.replace(r#""apiPath":"/v1/chat","#, "")),
        (
            "fullyFilteredEvents",
            good.replace(r#""fullyFilteredEvents":["bad"],"#, ""),
        ),
    ] {
        assert!(conf.len() < good.len(), "{field} was not removed");
        let mut sides = Vec::new();
        for ours in [false, true] {
            let a = format!("text-filtering-config={conf}");
            let props = [a.as_str(), "text-filtering-version=1"];
            let builder = if ours {
                crate::common::ours(7, "survival", &props)
            } else {
                harness::vanilla(7, "survival", &props)
            };
            let server = builder.start_unready().expect("spawn the server");
            let exit = server.wait_exit(Duration::from_secs(120));
            let log = server.log();
            server.kill();
            let exit = exit.expect("the server exits by itself").code();
            let rows: Vec<String> = log
                .lines()
                .filter(|l| !l.starts_with(char::is_whitespace))
                .filter(|l| l.contains("Failed to start") || l.contains("Exception"))
                .map(|l| l.split_once("] ").map_or(l, |(_, rest)| rest).to_owned())
                .collect();
            assert!(!log.contains("Done ("), "{field}: the server started");
            sides.push((exit, rows));
        }
        println!("{field}: {sides:?}");
        assert_eq!(
            sides[0], sides[1],
            "{field}: vanilla (left) vs ours (right)"
        );
    }
}

/// A configuration that does not make a filter: the warning, with the first line of the exception, at startup.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn v1_unusable_configurations_log_like_vanilla() {
    let pki = Pki::new();
    let good = config("http://127.0.0.1:9", &pki, "app.p12");
    std::fs::write(pki.path("not.p12"), "not a keystore").expect("file");
    for conf in [
        config("http://127.0.0.1:9", &pki, "missing.p12"),
        config("http://127.0.0.1:9", &pki, "not.p12"),
        good.replace(P12_PASSWORD, "wrong"),
        good.replace(r#""tenantId":"tenant-1""#, r#""tenantId":"""#),
    ] {
        let rows = super::text_filter_dual::config_rows(&conf, 1);
        println!("{conf}: {rows:?}");
        assert!(!rows.is_empty(), "no log line for {conf}");
    }
}
