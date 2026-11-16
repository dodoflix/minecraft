//! The Mojang Services profile keys on `mc-server`, against a mock discovery and key service that the server reads
//! through `MINECRAFT_API_DISCOVERY_HOST` (authlib's `minecraft.api.discovery.host`). Vanilla's key set cannot be pointed
//! at a mock without a JVM property, and the real service cannot be recorded, so the proof is the mock plus the
//! byte-level signature goldens: with the keys held the game thread answers at once, the status flag and the secure
//! chat enforcement stay off, a `ChatSessionUpdate` received meanwhile is handled when the keys arrive, and the flag
//! follows once they have.
use crate::common::*;
use mc_protocol::auth::{PublicKey, ServerKeyPair, public_key_from_der};
use mc_protocol::codec::Instant as WireInstant;
use mc_protocol::packets::play::{
    ProfilePublicKeyData, RemoteChatSessionData, ServerboundChatSessionUpdatePacket,
    ServerboundPlayPacket,
};
use mc_testbot::join::{BotOptions, status_ping};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::signature::{SignatureEncoding, Signer};
use sha1::Sha1;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A discovery document pointing at `/keys`, which serves `public_key` as the one player certificate key. The first
/// discovery request waits for a message on `release` when `hold` is set.
struct Mock {
    discovery_url: String,
    release: mpsc::Sender<()>,
}

fn mock_services(public_key: &[u8], hold: bool) -> Mock {
    use base64::Engine;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind the mock");
    let base = format!("http://{}", listener.local_addr().expect("address"));
    let key = base64::engine::general_purpose::STANDARD.encode(public_key);
    let (release, released) = mpsc::channel::<()>();
    let served = base.clone();
    std::thread::spawn(move || {
        let mut held = hold;
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
            } else {
                format!(r#"{{"playerCertificateKeys":[{{"publicKey":"{key}"}}]}}"#)
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    Mock {
        discovery_url: format!("{base}/discovery"),
        release,
    }
}

fn services_key() -> (SigningKey<Sha1>, Vec<u8>) {
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

/// The `ChatSessionUpdate` of a client whose profile key `signer` (the Services key) certified.
fn chat_session_update(signer: &SigningKey<Sha1>, profile_id: uuid::Uuid) -> ServerboundPlayPacket {
    let player_key = ServerKeyPair::generate().expect("a player key");
    let der = player_key.public_key_der().to_vec();
    let key: PublicKey = public_key_from_der(&der).expect("the player key");
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock")
        .as_millis() as i64
        + 86_400_000;
    let mut payload = profile_id.as_bytes().to_vec();
    payload.extend_from_slice(&expires_at.to_be_bytes());
    payload.extend_from_slice(&der);
    ServerboundPlayPacket::ChatSessionUpdate(ServerboundChatSessionUpdatePacket {
        chat_session: RemoteChatSessionData {
            session_id: uuid::Uuid::new_v4(),
            profile_public_key: ProfilePublicKeyData {
                expires_at: WireInstant(expires_at),
                key,
                key_signature: signer.sign(&payload).to_vec(),
            },
        },
    })
}

/// The server broadcast the player's chat session (`INITIALIZE_CHAT`).
fn has_chat_session(bot: &mc_testbot::Bot) -> bool {
    bot.log_text()
        .lines()
        .any(|l| l.contains("<- play/player_info_update") && l.contains("chat_session: Some"))
}

async fn enforces_secure_chat(port: u16) -> bool {
    let options = BotOptions::new("127.0.0.1", port, "Pinger");
    status_ping(&options, Default::default())
        .await
        .expect("status ping")
        .status
        .enforces_secure_chat
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn a_held_key_fetch_blocks_no_tick_and_the_session_is_handled_when_it_arrives() {
    let (signer, public_key) = services_key();
    let mock = mock_services(&public_key, true);
    let server = ours(12345, "creative", &[])
        .env("MINECRAFT_API_DISCOVERY_HOST", &mock.discovery_url)
        .start()
        .expect("start our server");
    let bot = join("127.0.0.1", server.port, "Signer").await;
    let profile_id = mc_core::uuid_util::create_offline_player_uuid("Signer");

    bot.send(chat_session_update(&signer, profile_id));
    // the game thread answers a command at once although the keys are still being fetched
    let since = bot.chat_len();
    let sent = Instant::now();
    bot.command("list");
    bot.wait_chat("players online", since, Duration::from_secs(10))
        .await
        .expect("the list reply");
    assert!(
        sent.elapsed() < Duration::from_secs(1),
        "the game thread waited for the key fetch: {:?}",
        sent.elapsed()
    );
    assert!(
        !has_chat_session(&bot),
        "the session was accepted before the keys were known"
    );

    assert!(
        !server.log().contains("Services are unavailable"),
        "the fetch timed out (5 s) before the test released it\n{}",
        server.log()
    );
    mock.release.send(()).expect("release the fetch");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !has_chat_session(&bot) {
        assert!(
            Instant::now() < deadline,
            "the parked session was not handled once the keys arrived\n{}",
            server.log()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        !server.log().contains("Ignoring chat session"),
        "{}",
        server.log()
    );
    bot.disconnect().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn the_status_flag_needs_the_services_keys() {
    let (_, public_key) = services_key();
    let mock = mock_services(&public_key, true);
    let server = ours(
        12345,
        "creative",
        &["online-mode=true", "enforce-secure-profile=true"],
    )
    .env("MINECRAFT_API_DISCOVERY_HOST", &mock.discovery_url)
    .start()
    .expect("start our server");
    assert!(
        !enforces_secure_chat(server.port).await,
        "the flag is set while the keys are unavailable"
    );

    assert!(
        !server.log().contains("Services are unavailable"),
        "the fetch timed out (5 s) before the test released it\n{}",
        server.log()
    );
    mock.release.send(()).expect("release the fetch");
    // the status is rebuilt every 5 seconds
    let deadline = Instant::now() + Duration::from_secs(40);
    while !enforces_secure_chat(server.port).await {
        assert!(
            Instant::now() < deadline,
            "the flag never followed the keys"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
