//! Real-network smoke test against the vanilla server (`ref/server.jar`): status ping, offline login
//! through configuration, and an online-mode encryption handshake. Needs java, `ref/server.jar` and `MC_ACCEPT_EULA=1`.
//! Run with `-- --nocapture` to see the packet-id log.

use crate::common::*;
use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::auth::{digest_server_id, encrypt_with_public_key, generate_secret};
use mc_protocol::packet::ConnectionState;
use mc_protocol::packets::{configuration::*, handshake::*, login::*, status::*};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn status_and_offline_login_to_configuration() {
    let server = start_vanilla(false);
    let port = server.port;

    // ---- status
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Status).await;
    send!(
        c,
        ServerboundStatusPacket::StatusRequest(ServerboundStatusRequestPacket)
    );
    let payload = read(&mut c).await;
    log_id(ConnectionState::Status, &payload);
    let ClientboundStatusPacket::StatusResponse(resp) =
        ClientboundStatusPacket::decode_payload(&payload).unwrap()
    else {
        panic!("expected status response")
    };
    println!("status: {}", resp.status.to_json_string().unwrap());
    assert_eq!(
        resp.status.version.as_ref().unwrap().protocol,
        PROTOCOL_VERSION
    );
    assert_eq!(resp.status.players.as_ref().unwrap().online, 0);
    send!(
        c,
        ServerboundStatusPacket::PingRequest(ServerboundPingRequestPacket {
            time: 0x1234_5678_9abc
        })
    );
    let payload = read(&mut c).await;
    log_id(ConnectionState::Status, &payload);
    assert_eq!(
        ClientboundStatusPacket::decode_payload(&payload).unwrap(),
        ClientboundStatusPacket::PongResponse(ClientboundPongResponsePacket {
            time: 0x1234_5678_9abc
        })
    );

    // ---- login (offline) -> configuration
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Login).await;
    let name = "SmokeBot";
    let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
    send!(
        c,
        ServerboundLoginPacket::Hello(ServerboundHelloPacket {
            name: name.into(),
            profile_id: uuid
        })
    );
    let profile = loop {
        let payload = read(&mut c).await;
        log_id(ConnectionState::Login, &payload);
        match ClientboundLoginPacket::decode_payload(&payload).unwrap() {
            ClientboundLoginPacket::LoginCompression(p) => {
                println!("compression threshold {}", p.compression_threshold);
                c.set_compression(p.compression_threshold, false);
            }
            ClientboundLoginPacket::LoginFinished(p) => break p.game_profile,
            other => panic!("unexpected login packet {other:?}"),
        }
    };
    assert_eq!(profile.name, name);
    send!(
        c,
        ServerboundLoginPacket::LoginAcknowledged(ServerboundLoginAcknowledgedPacket)
    );

    send!(
        c,
        ServerboundConfigurationPacket::ClientInformation(ServerboundClientInformationPacket {
            information: Default::default()
        })
    );
    let mut seen = Vec::new();
    loop {
        let payload = read(&mut c).await;
        log_id(ConnectionState::Configuration, &payload);
        let packet = ClientboundConfigurationPacket::decode_payload(&payload).unwrap();
        seen.push(packet.name());
        match packet {
            ClientboundConfigurationPacket::SelectKnownPacks(_) => {
                send!(
                    c,
                    ServerboundConfigurationPacket::SelectKnownPacks(ServerboundSelectKnownPacks {
                        known_packs: vec![]
                    })
                )
            }
            ClientboundConfigurationPacket::KeepAlive(p) => {
                send!(
                    c,
                    ServerboundConfigurationPacket::KeepAlive(ServerboundKeepAlivePacket {
                        id: p.id
                    })
                )
            }
            ClientboundConfigurationPacket::Ping(p) => {
                send!(
                    c,
                    ServerboundConfigurationPacket::Pong(ServerboundPongPacket { id: p.id })
                )
            }
            ClientboundConfigurationPacket::FinishConfiguration(_) => {
                send!(
                    c,
                    ServerboundConfigurationPacket::FinishConfiguration(
                        ServerboundFinishConfigurationPacket
                    )
                );
                break;
            }
            _ => {}
        }
    }
    for expected in [
        "minecraft:custom_payload",
        "minecraft:registry_data",
        "minecraft:update_tags",
        "minecraft:select_known_packs",
    ] {
        assert!(
            seen.contains(&expected),
            "never received {expected}; got {seen:?}"
        );
    }
    // The next packet is the first play packet (`login`, id 50).
    let payload = read(&mut c).await;
    log_id(ConnectionState::Play, &payload);
    assert_eq!(
        mc_protocol::codec::read_varint(&mut &payload[..]).unwrap(),
        50
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn online_mode_encryption_handshake() {
    let server = start_vanilla(true);
    let port = server.port;
    let mut c = connect(port).await;
    handshake(&mut c, port, ClientIntent::Login).await;
    send!(
        c,
        ServerboundLoginPacket::Hello(ServerboundHelloPacket {
            name: "SmokeBot".into(),
            profile_id: Uuid::nil()
        })
    );
    let payload = read(&mut c).await;
    log_id(ConnectionState::Login, &payload);
    let ClientboundLoginPacket::Hello(hello) =
        ClientboundLoginPacket::decode_payload(&payload).unwrap()
    else {
        panic!("expected hello")
    };
    assert!(hello.should_authenticate);
    let secret = generate_secret();
    println!(
        "server hash would be {}",
        digest_server_id(&hello.server_id, &secret, &hello.public_key)
    );
    let key = ServerboundKeyPacket {
        keybytes: encrypt_with_public_key(&hello.public_key, &secret).unwrap(),
        encrypted_challenge: encrypt_with_public_key(&hello.public_key, &hello.challenge).unwrap(),
    };
    send!(c, ServerboundLoginPacket::Key(key));
    c.enable_encryption(secret);
    // The server accepted our key (challenge + secret) iff it can talk to us over the encrypted
    // stream: it then fails the session-server check (no real Mojang account) with a login disconnect,
    // or, if the session server is unreachable, with an "authservers down" disconnect.
    let payload = read(&mut c).await;
    log_id(ConnectionState::Login, &payload);
    match ClientboundLoginPacket::decode_payload(&payload).unwrap() {
        ClientboundLoginPacket::LoginDisconnect(d) => {
            println!("disconnect: {}", d.reason.0.get_string())
        }
        ClientboundLoginPacket::LoginCompression(_) => {
            println!("(server proceeded without authentication failure)")
        }
        other => panic!("unexpected {other:?}"),
    }
}
