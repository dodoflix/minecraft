//! Login (offline + online-mode encryption against a mock session server), configuration sequence, and the
//! per-connection failure isolation of `ConnectionSender::send`.
use crate::net_common as common;

use common::*;
use mc_game::server::network::ConnectionSender;
use mc_game::server::network::Outbound;
use mc_game::server::network::connection::Payload;
use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::auth::{digest_server_id, encrypt_with_public_key, generate_secret};
use mc_protocol::codec::{Decode, DecodeResult, Encode, EncodeError, EncodeResult};
use mc_protocol::net::Connection;
use mc_protocol::packet::{ConnectionState, Packet, PacketFlow};
use mc_protocol::packets::common::ClientInformation;
use mc_protocol::packets::configuration::*;
use mc_protocol::packets::handshake::ClientIntent;
use mc_protocol::packets::login::*;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

fn login_payload(p: ServerboundLoginPacket) -> Vec<u8> {
    let mut buf = Vec::new();
    p.encode_payload(&mut buf).unwrap();
    buf
}

fn config_payload(p: ServerboundConfigurationPacket) -> Vec<u8> {
    let mut buf = Vec::new();
    p.encode_payload(&mut buf).unwrap();
    buf
}

async fn hello(c: &mut Connection<TcpStream>, name: &str) {
    write_payload(
        c,
        &login_payload(ServerboundLoginPacket::Hello(ServerboundHelloPacket {
            name: name.into(),
            profile_id: Uuid::nil(),
        })),
    )
    .await;
}

async fn read_login(c: &mut Connection<TcpStream>) -> ClientboundLoginPacket {
    ClientboundLoginPacket::decode_payload(&read(c).await).unwrap()
}

async fn read_config(c: &mut Connection<TcpStream>) -> ClientboundConfigurationPacket {
    ClientboundConfigurationPacket::decode_payload(&read(c).await).unwrap()
}

/// Offline handshake + hello; returns the connection in the login state after `LoginFinished` (compression active).
async fn offline_login(
    server: &TestServer,
    name: &str,
) -> (Connection<TcpStream>, ClientboundLoginFinishedPacket) {
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, name).await;
    let ClientboundLoginPacket::LoginCompression(comp) = read_login(&mut c).await else {
        panic!("expected login compression")
    };
    c.set_compression(comp.compression_threshold, true);
    let ClientboundLoginPacket::LoginFinished(finished) = read_login(&mut c).await else {
        panic!("expected login finished")
    };
    (c, finished)
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_login_uses_the_offline_uuid_and_the_compression_threshold() {
    let server = TestServer::start(MockHooks::new());
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let ClientboundLoginPacket::LoginCompression(comp) = read_login(&mut c).await else {
        panic!()
    };
    assert_eq!(comp.compression_threshold, 256);
    c.set_compression(256, true);
    let ClientboundLoginPacket::LoginFinished(finished) = read_login(&mut c).await else {
        panic!()
    };
    // UUID.nameUUIDFromBytes("OfflinePlayer:Steve"), the same value vanilla logs for an offline Steve.
    assert_eq!(
        finished.game_profile.id,
        mc_core::uuid_util::create_offline_player_uuid("Steve")
    );
    assert_eq!(
        finished.game_profile.id.to_string(),
        "5627dd98-e6be-3c21-b8a8-e92344183641"
    );
    assert_eq!(finished.game_profile.name, "Steve");
    assert!(finished.game_profile.properties.is_empty());
    assert!(
        server
            .hooks
            .log
            .lock()
            .contains(&"can_player_login Steve".to_owned())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn compression_off_sends_no_compression_packet() {
    let hooks = MockHooks::new();
    hooks.settings.lock().compression_threshold = -1;
    let server = TestServer::start(hooks);
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    assert!(matches!(
        read_login(&mut c).await,
        ClientboundLoginPacket::LoginFinished(_)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_usernames_and_protocol_errors_disconnect_with_vanilla_text() {
    let server = TestServer::start(MockHooks::new());
    for name in ["a b", "this_name_is_way_too_long", "caf\u{e9}"] {
        let mut c = connect(server.addr).await;
        handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
        // `ServerboundHelloPacket` reads the name with a 16-char limit: longer names fail at decode.
        hello(&mut c, name).await;
        let ClientboundLoginPacket::LoginDisconnect(d) = read_login(&mut c).await else {
            panic!("expected disconnect for {name:?}")
        };
        let text = d.reason.0.get_string();
        if name.chars().count() > 16 {
            assert!(text.starts_with("Internal Exception: io.netty.handler.codec.DecoderException: Failed to decode packet 'serverbound/minecraft:hello'"), "{text}");
        } else {
            assert_eq!(
                text,
                "Internal Exception: java.lang.IllegalStateException: Invalid characters in username"
            );
        }
        expect_closed(&mut c).await;
    }
    // Hello twice.
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    read_login(&mut c).await; // login compression
    c.set_compression(256, true);
    hello(&mut c, "Steve").await;
    // (login finished may still be in flight; skip to the disconnect)
    loop {
        match read_login(&mut c).await {
            ClientboundLoginPacket::LoginDisconnect(d) => {
                assert_eq!(
                    d.reason.0.get_string(),
                    "Internal Exception: java.lang.IllegalStateException: Unexpected hello packet"
                );
                break;
            }
            _ => continue,
        }
    }
    // Acknowledgement before login finished.
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    let ClientboundLoginPacket::LoginDisconnect(d) = read_login(&mut c).await else {
        panic!()
    };
    assert_eq!(
        d.reason.0.get_string(),
        "Internal Exception: java.lang.IllegalStateException: Unexpected login acknowledgement packet"
    );
    // A cookie response in the login phase.
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::CookieResponse(
            mc_protocol::packets::login::ServerboundCookieResponsePacket {
                key: mc_core::ident::Identifier::with_default_namespace("x").unwrap(),
                payload: None,
            },
        )),
    )
    .await;
    let ClientboundLoginPacket::LoginDisconnect(d) = read_login(&mut c).await else {
        panic!()
    };
    assert_eq!(
        d.reason.0.get_string(),
        "Unexpected custom data from client"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn player_list_refusal_disconnects() {
    let hooks = MockHooks::new();
    *hooks.login_error.lock() = Some(mc_text::Component::literal("You are banned"));
    let server = TestServer::start(hooks);
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let ClientboundLoginPacket::LoginDisconnect(d) = read_login(&mut c).await else {
        panic!()
    };
    assert_eq!(d.reason.0.get_string(), "You are banned");
    expect_closed(&mut c).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_login_waits_until_the_old_player_is_gone() {
    let hooks = MockHooks::new();
    let id = mc_core::uuid_util::create_offline_player_uuid("Steve");
    hooks.online.lock().insert(id);
    let server = TestServer::start(hooks.clone());
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let ClientboundLoginPacket::LoginCompression(_) = read_login(&mut c).await else {
        panic!()
    };
    c.set_compression(256, true);
    // WAITING_FOR_DUPE_DISCONNECT: nothing until the player is removed.
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(400), c.read_payload())
            .await
            .is_err()
    );
    hooks.online.lock().remove(&id);
    assert!(matches!(
        read_login(&mut c).await,
        ClientboundLoginPacket::LoginFinished(_)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn configuration_sequence_and_hand_off() {
    let hooks = MockHooks::new();
    let server = TestServer::start(hooks.clone());
    let (mut c, finished) = offline_login(&server, "Steve").await;
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;

    // vanilla 26.3 order: brand, enabled features, known packs (then, after the answer, registries, tags, finish)
    let ClientboundConfigurationPacket::CustomPayload(brand) = read_config(&mut c).await else {
        panic!("brand")
    };
    assert_eq!(brand.payload.as_brand().as_deref(), Some("vanilla"));
    assert!(matches!(
        read_config(&mut c).await,
        ClientboundConfigurationPacket::UpdateEnabledFeatures(_)
    ));
    let ClientboundConfigurationPacket::SelectKnownPacks(packs) = read_config(&mut c).await else {
        panic!("known packs")
    };
    assert_eq!(packs.known_packs, hooks.configuration.lock().known_packs);

    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::ClientInformation(
            ServerboundClientInformationPacket {
                information: ClientInformation {
                    language: "de_de".into(),
                    view_distance: 7,
                    ..ClientInformation::default()
                },
            },
        )),
    )
    .await;
    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::SelectKnownPacks(
            ServerboundSelectKnownPacks {
                known_packs: packs.known_packs.clone(),
            },
        )),
    )
    .await;
    assert!(matches!(
        read_config(&mut c).await,
        ClientboundConfigurationPacket::UpdateTags(_)
    ));
    assert!(matches!(
        read_config(&mut c).await,
        ClientboundConfigurationPacket::FinishConfiguration(_)
    ));
    assert!(hooks.spawned.lock().is_empty());
    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::FinishConfiguration(
            ServerboundFinishConfigurationPacket,
        )),
    )
    .await;

    let pending = loop {
        if let Some(p) = hooks.spawned.lock().pop() {
            break p;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    assert_eq!(pending.cookie.profile, finished.game_profile);
    assert_eq!(pending.cookie.client_information.language, "de_de");
    assert_eq!(pending.cookie.client_information.view_distance, 7);
    assert!(!pending.cookie.transferred);
    // Closing the client ends the inbound channel with the disconnect event.
    drop(c);
    let event = tokio::task::spawn_blocking(move || {
        pending
            .inbound
            .recv_timeout(std::time::Duration::from_secs(10))
    })
    .await
    .unwrap()
    .unwrap();
    match event {
        mc_game::server::network::InboundPacket::Disconnected(d) => {
            assert_eq!(d.reason.get_string(), "End of stream")
        }
        _ => panic!("expected the disconnect event"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_answers_in_configuration_are_rejected() {
    let server = TestServer::start(MockHooks::new());
    let (mut c, _) = offline_login(&server, "Steve").await;
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    for _ in 0..3 {
        read_config(&mut c).await;
    }
    // Finish before the registry negotiation completed: "Unexpected request for task finish".
    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::FinishConfiguration(
            ServerboundFinishConfigurationPacket,
        )),
    )
    .await;
    let ClientboundConfigurationPacket::Disconnect(d) = read_config(&mut c).await else {
        panic!("expected disconnect")
    };
    assert_eq!(
        d.reason.get_string(),
        "Internal Exception: java.lang.IllegalStateException: Unexpected request for task finish, current task: synchronize_registries, requested: join_world"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_login_packet_closes_only_that_connection() {
    let server = TestServer::start(MockHooks::new());
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    // `Hello` whose name claims 1000 bytes (the limit is 16 characters).
    let mut hello = vec![0x00, 0xE8, 0x07];
    hello.extend(std::iter::repeat_n(b'a', 1000));
    hello.extend([0u8; 16]);
    write_payload(&mut c, &hello).await;
    assert!(matches!(
        read_login(&mut c).await,
        ClientboundLoginPacket::LoginDisconnect(_)
    ));
    expect_closed(&mut c).await;
    expect_server_answers_status(server.addr).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_configuration_packet_closes_only_that_connection() {
    let server = TestServer::start(MockHooks::new());
    let (mut c, _) = offline_login(&server, "Steve").await;
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    for _ in 0..3 {
        read_config(&mut c).await;
    }
    // Unknown packet id 0x7f in the configuration state.
    write_payload(&mut c, &[0x7f]).await;
    assert!(matches!(
        read_config(&mut c).await,
        ClientboundConfigurationPacket::Disconnect(_)
    ));
    expect_closed(&mut c).await;
    expect_server_answers_status(server.addr).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn resource_pack_and_code_of_conduct_tasks() {
    use mc_game::server::network::hooks::ServerResourcePackInfo;
    let hooks = MockHooks::new();
    {
        let mut cfg = default_configuration();
        cfg.code_of_conducts
            .insert("en_us".into(), "be nice".into());
        cfg.code_of_conducts
            .insert("de_de".into(), "sei nett".into());
        cfg.resource_pack = Some(ServerResourcePackInfo {
            id: Uuid::from_u128(7),
            url: "http://example.invalid/pack.zip".into(),
            hash: "0".repeat(40),
            is_required: true,
            prompt: None,
        });
        *hooks.configuration.lock() = Arc::new(cfg);
    }
    let server = TestServer::start(hooks.clone());
    let (mut c, _) = offline_login(&server, "Steve").await;
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    for _ in 0..2 {
        read_config(&mut c).await;
    }
    let ClientboundConfigurationPacket::SelectKnownPacks(packs) = read_config(&mut c).await else {
        panic!()
    };
    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::ClientInformation(
            ServerboundClientInformationPacket {
                information: ClientInformation {
                    language: "DE_DE".into(),
                    ..ClientInformation::default()
                },
            },
        )),
    )
    .await;
    // Information arrives before the pack answer, so the code of conduct picks the client's language (lower-cased).
    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::SelectKnownPacks(
            ServerboundSelectKnownPacks {
                known_packs: packs.known_packs,
            },
        )),
    )
    .await;
    assert!(matches!(
        read_config(&mut c).await,
        ClientboundConfigurationPacket::UpdateTags(_)
    ));
    let ClientboundConfigurationPacket::CodeOfConduct(coc) = read_config(&mut c).await else {
        panic!("code of conduct")
    };
    assert_eq!(coc.code_of_conduct, "sei nett");
    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::AcceptCodeOfConduct(
            ServerboundAcceptCodeOfConductPacket,
        )),
    )
    .await;
    let ClientboundConfigurationPacket::ResourcePackPush(push) = read_config(&mut c).await else {
        panic!("resource pack")
    };
    assert_eq!(push.id, Uuid::from_u128(7));
    assert!(push.required);
    // Declining a required pack kicks.
    write_payload(
        &mut c,
        &config_payload(ServerboundConfigurationPacket::ResourcePack(
            ServerboundResourcePackPacket {
                id: push.id,
                action: mc_protocol::packets::common::ResourcePackAction::Declined,
            },
        )),
    )
    .await;
    let ClientboundConfigurationPacket::Disconnect(d) = read_config(&mut c).await else {
        panic!("expected disconnect")
    };
    assert_eq!(
        d.reason.get_string(),
        "This server requires a custom resource pack"
    );
}

// ---- online mode -------------------------------------------------------------------------------------------------

/// One-shot HTTP responder: returns the request line it saw.
async fn mock_session_server(
    body: &'static str,
    status_line: &'static str,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = s.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        let response = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(response.as_bytes()).await.unwrap();
        request.lines().next().unwrap_or_default().to_owned()
    });
    (base, task)
}

async fn online_key_exchange(c: &mut Connection<TcpStream>, name: &str) -> [u8; 16] {
    handshake(c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(c, name).await;
    let ClientboundLoginPacket::Hello(h) = read_login(c).await else {
        panic!("expected hello")
    };
    assert_eq!(h.server_id, "");
    assert!(h.should_authenticate);
    assert_eq!(h.challenge.len(), 4);
    let secret = generate_secret();
    write_payload(
        c,
        &login_payload(ServerboundLoginPacket::Key(ServerboundKeyPacket {
            keybytes: encrypt_with_public_key(&h.public_key, &secret).unwrap(),
            encrypted_challenge: encrypt_with_public_key(&h.public_key, &h.challenge).unwrap(),
        })),
    )
    .await;
    c.enable_encryption(secret);
    let _ = digest_server_id("", &secret, &h.public_key);
    secret
}

#[tokio::test(flavor = "multi_thread")]
async fn online_mode_verifies_with_the_session_server() {
    let (base, request) = mock_session_server(
        r#"{"id":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"Notch","properties":[{"name":"textures","value":"abc","signature":"sig"}]}"#,
        "200 OK",
    )
    .await;
    let hooks = MockHooks::new();
    {
        let mut s = hooks.settings.lock();
        s.uses_authentication = true;
        s.prevent_proxy_connections = true;
    }
    *hooks.session.lock() = mc_protocol::auth::SessionClient::new(base);
    let server = TestServer::start(hooks.clone());
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let ClientboundLoginPacket::Hello(h) = read_login(&mut c).await else {
        panic!()
    };
    let secret = generate_secret();
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::Key(ServerboundKeyPacket {
            keybytes: encrypt_with_public_key(&h.public_key, &secret).unwrap(),
            encrypted_challenge: encrypt_with_public_key(&h.public_key, &h.challenge).unwrap(),
        })),
    )
    .await;
    c.enable_encryption(secret);
    let digest = digest_server_id("", &secret, &h.public_key);
    let ClientboundLoginPacket::LoginCompression(_) = read_login(&mut c).await else {
        panic!("expected compression")
    };
    c.set_compression(256, true);
    let ClientboundLoginPacket::LoginFinished(f) = read_login(&mut c).await else {
        panic!("expected finished")
    };
    assert_eq!(
        f.game_profile.id.to_string(),
        "069a79f4-44e9-4726-a5be-fca90e38aaf5"
    );
    assert_eq!(f.game_profile.name, "Steve"); // the requested name, like authlib's
    assert_eq!(f.game_profile.properties.len(), 1);
    let line = request.await.unwrap();
    assert!(
        line.contains(&format!("serverId={digest}"))
            && line.contains("username=Steve")
            && line.contains("ip=127.0.0.1"),
        "{line}"
    );
    assert!(hooks.log.lock().contains(&"login_activity".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn online_mode_rejects_unverified_sessions_and_bad_keys() {
    let (base, _) = mock_session_server("", "204 No Content").await;
    let hooks = MockHooks::new();
    hooks.settings.lock().uses_authentication = true;
    *hooks.session.lock() = mc_protocol::auth::SessionClient::new(base);
    let server = TestServer::start(hooks.clone());
    let mut c = connect(server.addr).await;
    online_key_exchange(&mut c, "Steve").await;
    let ClientboundLoginPacket::LoginDisconnect(d) = read_login(&mut c).await else {
        panic!("expected disconnect")
    };
    assert_eq!(d.reason.0.get_string(), "Failed to verify username!");

    // Session server unreachable: authservers_down.
    *hooks.session.lock() = mc_protocol::auth::SessionClient::new("http://127.0.0.1:1");
    let mut c = connect(server.addr).await;
    online_key_exchange(&mut c, "Steve").await;
    let ClientboundLoginPacket::LoginDisconnect(d) = read_login(&mut c).await else {
        panic!()
    };
    assert_eq!(
        d.reason.0.get_string(),
        "Authentication servers are down. Please try again later. Sorry!"
    );

    // Wrong challenge: Protocol error.
    let mut c = connect(server.addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let ClientboundLoginPacket::Hello(h) = read_login(&mut c).await else {
        panic!()
    };
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::Key(ServerboundKeyPacket {
            keybytes: encrypt_with_public_key(&h.public_key, &generate_secret()).unwrap(),
            encrypted_challenge: encrypt_with_public_key(&h.public_key, &[1, 2, 3, 4]).unwrap(),
        })),
    )
    .await;
    let ClientboundLoginPacket::LoginDisconnect(d) = read_login(&mut c).await else {
        panic!()
    };
    assert_eq!(
        d.reason.0.get_string(),
        "Internal Exception: java.lang.IllegalStateException: Protocol error"
    );
}

// ---- failure isolation ---------------------------------------------------------------------------------------------

/// A packet whose encoder refuses its data like vanilla codecs that throw `EncoderException`.
struct Unencodable<const SKIPPABLE: bool>;
impl<const SKIPPABLE: bool> Encode for Unencodable<SKIPPABLE> {
    fn encode(&self, _out: &mut Vec<u8>) -> EncodeResult<()> {
        Err(EncodeError::msg(
            "String too big (was 32768 characters, max 32767)",
        ))
    }
}
impl<const SKIPPABLE: bool> Decode for Unencodable<SKIPPABLE> {
    fn decode(_input: &mut &[u8]) -> DecodeResult<Self> {
        Ok(Self)
    }
}
impl<const SKIPPABLE: bool> Packet for Unencodable<SKIPPABLE> {
    const ID: i32 = 0x7e;
    const NAME: &'static str = "minecraft:unencodable";
    const STATE: ConnectionState = ConnectionState::Play;
    const FLOW: PacketFlow = PacketFlow::Clientbound;
    const SKIPPABLE: bool = SKIPPABLE;
}

#[test]
fn an_encoder_failure_disconnects_only_that_connection() {
    let (tx_bad, mut rx_bad) = tokio::sync::mpsc::unbounded_channel();
    let (tx_ok, mut rx_ok) = tokio::sync::mpsc::unbounded_channel();
    let (tx_other, mut rx_other) = tokio::sync::mpsc::unbounded_channel();
    let bad = ConnectionSender::new(tx_bad);
    let ok = ConnectionSender::new(tx_ok);
    let other = ConnectionSender::new(tx_other);
    bad.send(&Unencodable::<false>);
    match rx_bad.try_recv() {
        Ok(Outbound::Disconnect(reason)) => assert_eq!(
            reason.get_string(),
            "Internal Exception: io.netty.handler.codec.EncoderException: Failed to encode packet 'clientbound/minecraft:unencodable'"
        ),
        _ => panic!("the failing connection must be disconnected"),
    }
    assert!(bad.handle().is_read_only());
    // The neighbour is untouched and still usable.
    ok.send(&ClientboundFinishConfigurationPacket);
    assert!(matches!(rx_ok.try_recv(), Ok(Outbound::Payload(_))));
    assert!(!ok.handle().is_read_only());

    // A packet encoded once for a broadcast fails on every recipient that gets it, and only on those.
    let broadcast = Payload::encode(&Unencodable::<false>);
    other.send_payload(broadcast.clone());
    assert!(matches!(rx_other.try_recv(), Ok(Outbound::Disconnect(_))));
    ok.send_payload(Payload::encode(&ClientboundFinishConfigurationPacket));
    assert!(matches!(rx_ok.try_recv(), Ok(Outbound::Payload(_))));
}

#[test]
fn a_skippable_packet_that_fails_to_encode_is_dropped() {
    // `Packet.isSkippable()` (chat, combat kill, tag query): `SkipPacketEncoderException` = logged and skipped.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let conn = ConnectionSender::new(tx);
    conn.send(&Unencodable::<true>);
    assert!(
        rx.try_recv().is_err(),
        "nothing is queued and nobody is kicked"
    );
    assert!(!conn.handle().is_read_only());
    conn.send(&ClientboundFinishConfigurationPacket);
    assert!(matches!(rx.try_recv(), Ok(Outbound::Payload(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn packet_rate_limit_kicks_spammers() {
    let hooks = MockHooks::new();
    hooks.settings.lock().rate_limit_packets_per_second = 5;
    let server = TestServer::start(hooks);
    let (mut c, _) = offline_login(&server, "Spammer").await;
    write_payload(
        &mut c,
        &login_payload(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    let info = config_payload(ServerboundConfigurationPacket::ClientInformation(
        ServerboundClientInformationPacket {
            information: ClientInformation::default(),
        },
    ));
    // 400 packets within the first second: the smoothed average (0.25 * 400) is far above 5 at the next once-a-second tick.
    for _ in 0..400 {
        c.write_payload(&info).await.unwrap();
    }
    c.flush().await.unwrap();
    loop {
        if let ClientboundConfigurationPacket::Disconnect(d) = read_config(&mut c).await {
            assert_eq!(
                d.reason.get_string(),
                "Kicked for exceeding packet rate limit"
            );
            break;
        }
    }
    expect_closed(&mut c).await;
}
