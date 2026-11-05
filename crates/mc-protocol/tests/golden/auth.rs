//! Golden tests against vanilla (`tests/fixtures/AuthGolden.java`, `AuthDecrypt.java`) and the
//! `hasJoined` client against a local mock session server.

use std::path::PathBuf;
use std::process::Command;

use mc_protocol::auth::*;
use mc_protocol::codec::{Decode, DecodeError, Encode, Property};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const GOLDEN: &str = include_str!("../fixtures/auth_golden.txt");

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

fn golden(key: &str) -> Vec<u8> {
    let line = GOLDEN.lines().find_map(|l| l.strip_prefix(&format!("{key}="))).expect("fixture key");
    hex(line)
}

fn server_key() -> ServerKeyPair {
    ServerKeyPair::from_pkcs8_der(&golden("private_pkcs8")).unwrap()
}

#[test]
fn public_key_der_matches_vanilla() {
    let kp = server_key();
    assert_eq!(kp.public_key_der(), golden("public_der"));
    assert_eq!(kp.public_key_der().len(), 162); // RSA-1024, e = 65537
}

#[test]
fn generated_key_is_rsa_1024_and_roundtrips() {
    let kp = ServerKeyPair::generate().unwrap();
    assert_eq!(kp.public_key_der().len(), 162);
    let secret = generate_secret();
    let ct = encrypt_with_public_key(kp.public_key_der(), &secret).unwrap();
    assert_eq!(ct.len(), 128);
    assert_eq!(kp.decrypt_secret_key(&ct).unwrap(), secret);
    assert_ne!(generate_secret(), generate_secret());
}

#[test]
fn decrypts_vanilla_ciphertext() {
    let kp = server_key();
    assert_eq!(kp.decrypt(&golden("challenge_ct")).unwrap(), golden("challenge"));
    assert_eq!(kp.decrypt_secret_key(&golden("secret_ct")).unwrap().to_vec(), golden("secret"));
    assert_eq!(kp.decrypt_secret_key(&golden("secret_ct_stripped")).unwrap().to_vec(), golden("secret"));
}

#[test]
fn wrong_secret_length_and_garbage_fail() {
    let kp = server_key();
    assert!(matches!(kp.decrypt_secret_key(&golden("secret24_ct")), Err(CryptError::SecretKeyLength(24))));
    assert!(matches!(kp.decrypt_secret_key(&golden("empty_ct")), Err(CryptError::SecretKeyLength(0))));
    assert!(kp.decrypt(&[1, 2, 3]).is_err());
    assert!(kp.decrypt(&[0xff; 129]).is_err());
}

#[test]
fn vanilla_decrypts_rust_ciphertext() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/vanilla-java.sh");
    if !script.exists() {
        eprintln!("skipped: vanilla oracle script missing");
        return;
    }
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/AuthDecrypt.java");
    let key = golden("public_der");
    for plain in [&golden("secret")[..], &[7u8; 117], &[]] {
        let ct = encrypt_with_public_key(&key, plain).unwrap();
        let out = Command::new(&script)
            .arg(&fixture)
            .arg(to_hex(&golden("private_pkcs8")))
            .arg(to_hex(&ct))
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), to_hex(plain));
    }
    // Too long for PKCS#1 v1.5 with a 1024-bit key, like Java's "Data must not be longer than 117 bytes".
    assert!(encrypt_with_public_key(&key, &[0; 118]).is_err());
}

#[test]
fn digest_matches_wiki_vectors() {
    assert_eq!(digest_server_id("Notch", &[], &[]), "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48");
    assert_eq!(digest_server_id("jeb_", &[], &[]), "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1");
    assert_eq!(digest_server_id("simon", &[], &[]), "88e16a1019277b15d58faf0541e11910eb756f6");
}

#[test]
fn digest_matches_vanilla() {
    let mut n = 0;
    let (mut neg, mut short, mut non_ascii) = (0, 0, 0);
    for line in GOLDEN.lines().filter_map(|l| l.strip_prefix("digest=")) {
        let [id, secret, key, want] = line.split('|').collect::<Vec<_>>()[..] else { panic!("bad line {line}") };
        assert_eq!(digest_server_id(id, &hex(secret), &hex(key)), want, "{line}");
        n += 1;
        neg += want.starts_with('-') as u32;
        short += (want.trim_start_matches('-').len() < 40) as u32;
        non_ascii += !id.is_ascii() as u32;
    }
    assert_eq!(n, 200);
    assert!(neg > 0 && short > 0 && non_ascii > 0);
}

#[test]
fn public_key_codec() {
    let wire = golden("public_wire");
    let key = PublicKey::decode(&mut &wire[..]).unwrap();
    assert_eq!(key.der(), golden("public_der"));
    let mut out = Vec::new();
    key.encode(&mut out).unwrap();
    assert_eq!(out, wire);
    assert_eq!(key, public_key_from_der(&golden("public_der")).unwrap());

    // encrypt via the wire key, decrypt with the server key
    let ct = key.encrypt(&golden("secret")).unwrap();
    assert_eq!(server_key().decrypt(&ct).unwrap(), golden("secret"));
}

#[test]
fn public_key_codec_errors() {
    // varint 3, garbage
    assert_eq!(
        PublicKey::decode(&mut &[3u8, 1, 2, 3][..]).unwrap_err(),
        DecodeError::msg("Malformed public key bytes")
    );
    // varint 513 = 0x81 0x04
    assert_eq!(
        PublicKey::decode(&mut &[0x81u8, 0x04][..]).unwrap_err(),
        DecodeError::msg("ByteArray with size 513 is bigger than allowed 512")
    );
    assert!(matches!(PublicKey::decode(&mut &[0x05u8, 1][..]), Err(DecodeError::Eof { .. })));
}

/// Serves one canned HTTP response; resolves to the request line it received.
async fn mock(response: &'static str) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut chunk = [0u8; 1024];
            let n = sock.read(&mut chunk).await.unwrap();
            assert!(n > 0, "client closed early");
            buf.extend_from_slice(&chunk[..n]);
        }
        sock.write_all(response.as_bytes()).await.unwrap();
        sock.shutdown().await.unwrap();
        String::from_utf8(buf).unwrap().lines().next().unwrap().to_owned()
    });
    (base, task)
}

fn json_response(body: &str) -> &'static str {
    Box::leak(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_boxed_str(),
    )
}

#[tokio::test]
async fn has_joined_returns_profile() {
    let body = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[
        {"name":"textures","value":"dGV4","signature":"c2ln"},{"name":"plain","value":"v"}],
        "profileActions":[]}"#;
    let (base, req) = mock(json_response(body)).await;
    let got = SessionClient::new(base).has_joined("Notch", "-1a2b", Some("127.0.0.1")).await.unwrap().unwrap();
    assert_eq!(req.await.unwrap(), "GET /session/minecraft/hasJoined?ip=127.0.0.1&serverId=-1a2b&username=Notch HTTP/1.1");
    assert_eq!(got.id.to_string(), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
    assert_eq!(got.name, "Notch");
    assert_eq!(
        got.properties,
        vec![
            Property { name: "textures".into(), value: "dGV4".into(), signature: Some("c2ln".into()) },
            Property { name: "plain".into(), value: "v".into(), signature: None },
        ]
    );
}

#[tokio::test]
async fn has_joined_encodes_query_like_authlib_and_omits_ip() {
    let (base, req) = mock(json_response(r#"{"id":"069a79f4-44e9-4726-a5be-fca90e38aaf5"}"#)).await;
    // trailing slash on the base URL is tolerated
    let got = SessionClient::new(format!("{base}/")).has_joined("Nötch *~ a+b", "-1a2b", None).await.unwrap().unwrap();
    // expected string printed by tests/fixtures/AuthQuery.java (authlib HttpDiscoveryService.buildQuery)
    assert_eq!(
        req.await.unwrap(),
        "GET /session/minecraft/hasJoined?serverId=-1a2b&username=N%C3%B6tch+*%7E+a%2Bb HTTP/1.1"
    );
    assert_eq!(got.name, "Nötch *~ a+b");
    assert!(got.properties.is_empty());
}

#[tokio::test]
async fn has_joined_204_is_none() {
    let (base, _) = mock("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n").await;
    assert!(SessionClient::new(base).has_joined("a", "b", None).await.unwrap().is_none());
}

#[tokio::test]
async fn has_joined_empty_200_and_bad_json_are_none() {
    let (base, _) = mock(json_response("")).await;
    assert!(SessionClient::new(base).has_joined("a", "b", None).await.unwrap().is_none());
    let (base, _) = mock(json_response("{\"id\":null}")).await;
    assert!(SessionClient::new(base).has_joined("a", "b", None).await.unwrap().is_none());
    let (base, _) = mock(json_response("not json")).await;
    assert!(SessionClient::new(base).has_joined("a", "b", None).await.unwrap().is_none());
}

#[tokio::test]
async fn has_joined_4xx_is_none_5xx_is_error() {
    let (base, _) = mock("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    assert!(SessionClient::new(base).has_joined("a", "b", None).await.unwrap().is_none());
    let (base, _) = mock("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    assert!(matches!(SessionClient::new(base).has_joined("a", "b", None).await, Err(AuthError::Status(500))));
}

#[tokio::test]
async fn has_joined_unreachable_is_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    assert!(matches!(SessionClient::new(base).has_joined("a", "b", None).await, Err(AuthError::Io(_))));
}
