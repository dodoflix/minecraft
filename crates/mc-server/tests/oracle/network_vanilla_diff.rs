//! Differential test: the same client scenarios against the real vanilla server (`ref/server.jar`) and against our
//! listener (mock hooks configured like a default offline vanilla server) and against our `mc-server` binary; transcripts
//! must be identical. Needs java, the jar and `MC_ACCEPT_EULA=1` (`docs/testing.md`); without them the tests fail.
use crate::net_support::*;
use mc_game::server::network::hooks::RegistrySynchronization;
use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::codec::read_varint;
use mc_protocol::net::Connection;
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::common::ClientInformation;
use mc_protocol::packets::configuration::*;
use mc_protocol::packets::handshake::ClientIntent;
use mc_protocol::packets::login::*;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

fn name_of(state: ConnectionState, payload: &[u8]) -> String {
    let id = read_varint(&mut &payload[..]).unwrap();
    packet_name(state, PacketFlow::Clientbound, id)
        .unwrap_or("?")
        .to_owned()
}

fn login_bytes(p: ServerboundLoginPacket) -> Vec<u8> {
    let mut b = Vec::new();
    p.encode_payload(&mut b).unwrap();
    b
}

fn config_bytes(p: ServerboundConfigurationPacket) -> Vec<u8> {
    let mut b = Vec::new();
    p.encode_payload(&mut b).unwrap();
    b
}

async fn hello(c: &mut Connection<TcpStream>, name: &str) {
    write_payload(
        c,
        &login_bytes(ServerboundLoginPacket::Hello(ServerboundHelloPacket {
            name: name.into(),
            profile_id: Uuid::nil(),
        })),
    )
    .await;
}

/// Raw payload of the one packet the server sends before closing (hex), or `closed`.
async fn single_reply(c: &mut Connection<TcpStream>) -> String {
    match tokio::time::timeout(Duration::from_secs(10), c.read_payload()).await {
        Ok(Ok(p)) => hex(&p),
        Ok(Err(_)) => "closed".into(),
        Err(_) => "timeout".into(),
    }
}

async fn legacy(addr: SocketAddr, request: &[u8]) -> String {
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(request).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), s.read_to_end(&mut buf)).await;
    hex(&buf)
}

/// Runs every scenario against `addr`; the transcript is what must match.
async fn transcript(addr: SocketAddr) -> Vec<String> {
    let mut t = Vec::new();

    // legacy pings: 1.4-1.5, 1.6 (MC|PingHost), pre-1.4
    t.push(format!("legacy0 {}", legacy(addr, &[0xfe]).await));
    t.push(format!("legacy1 {}", legacy(addr, &[0xfe, 0x01]).await));
    let utf16 = |s: &str| {
        s.encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<u8>>()
    };
    let mut payload = vec![78u8];
    payload.extend(9u16.to_be_bytes());
    payload.extend(utf16("localhost"));
    payload.extend(25565i32.to_be_bytes());
    let mut req = vec![0xfe, 0x01, 0xfa];
    req.extend(11u16.to_be_bytes());
    req.extend(utf16("MC|PingHost"));
    req.extend((payload.len() as u16).to_be_bytes());
    req.extend(payload);
    t.push(format!("legacy16 {}", legacy(addr, &req).await));

    // handshake rejects
    for (protocol, intent) in [
        (753, ClientIntent::Login),
        (754, ClientIntent::Login),
        (PROTOCOL_VERSION + 1, ClientIntent::Login),
        (10, ClientIntent::Transfer),
    ] {
        let mut c = connect(addr).await;
        handshake(&mut c, protocol, intent).await;
        t.push(format!(
            "handshake {protocol} {intent:?} {}",
            single_reply(&mut c).await
        ));
    }

    // login errors
    for name in ["a b", "caf\u{e9}", "this_name_is_way_too_long"] {
        let mut c = connect(addr).await;
        handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
        hello(&mut c, name).await;
        t.push(format!("hello {name:?} {}", single_reply(&mut c).await));
    }
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    write_payload(
        &mut c,
        &login_bytes(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    t.push(format!("early ack {}", single_reply(&mut c).await));
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    write_payload(
        &mut c,
        &login_bytes(ServerboundLoginPacket::CookieResponse(
            mc_protocol::packets::login::ServerboundCookieResponsePacket {
                key: mc_core::ident::Identifier::with_default_namespace("x").unwrap(),
                payload: None,
            },
        )),
    )
    .await;
    t.push(format!("cookie {}", single_reply(&mut c).await));
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    write_payload(
        &mut c,
        &login_bytes(ServerboundLoginPacket::Key(ServerboundKeyPacket {
            keybytes: vec![1],
            encrypted_challenge: vec![2],
        })),
    )
    .await;
    t.push(format!("early key {}", single_reply(&mut c).await));

    // full offline flow up to the first play packet
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let p = read(&mut c).await;
    t.push(format!("login {}", hex(&p)));
    c.set_compression(256, true);
    let p = read(&mut c).await;
    let ClientboundLoginPacket::LoginFinished(f) =
        ClientboundLoginPacket::decode_payload(&p).unwrap()
    else {
        panic!("finished")
    };
    t.push(format!(
        "finished {} {} {:?}",
        f.game_profile.id, f.game_profile.name, f.game_profile.properties
    ));
    write_payload(
        &mut c,
        &login_bytes(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    let mut registry_run = false;
    loop {
        let p = read(&mut c).await;
        let name = name_of(ConnectionState::Configuration, &p);
        match name.as_str() {
            "minecraft:registry_data" => {
                if !registry_run {
                    t.push("config minecraft:registry_data*".into());
                    registry_run = true;
                }
                continue;
            }
            "minecraft:custom_payload"
            | "minecraft:update_enabled_features"
            | "minecraft:select_known_packs" => {
                t.push(format!("config {name} {}", hex(&p)));
                if name == "minecraft:select_known_packs" {
                    let ClientboundConfigurationPacket::SelectKnownPacks(k) =
                        ClientboundConfigurationPacket::decode_payload(&p).unwrap()
                    else {
                        panic!()
                    };
                    let known = k.known_packs;
                    write_payload(
                        &mut c,
                        &config_bytes(ServerboundConfigurationPacket::ClientInformation(
                            ServerboundClientInformationPacket {
                                information: ClientInformation::default(),
                            },
                        )),
                    )
                    .await;
                    write_payload(
                        &mut c,
                        &config_bytes(ServerboundConfigurationPacket::SelectKnownPacks(
                            ServerboundSelectKnownPacks { known_packs: known },
                        )),
                    )
                    .await;
                }
            }
            _ => t.push(format!("config {name}")),
        }
        if name == "minecraft:finish_configuration" {
            break;
        }
    }
    t
}

struct SomeRegistries;
impl RegistrySynchronization for SomeRegistries {
    fn pack_registries(
        &self,
        _negotiated: &std::collections::HashSet<KnownPack>,
    ) -> Vec<ClientboundRegistryDataPacket> {
        (0..3)
            .map(|i| ClientboundRegistryDataPacket {
                registry: mc_core::ident::Identifier::with_default_namespace(&format!("r{i}"))
                    .unwrap(),
                entries: vec![],
            })
            .collect()
    }
    fn tags(&self) -> ClientboundUpdateTagsPacket {
        ClientboundUpdateTagsPacket { tags: vec![] }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn transcripts_equal_the_real_vanilla_server() {
    let vanilla = mc_testkit::Server::vanilla()
        .properties(PROPERTIES)
        .start()
        .expect("start vanilla");
    let theirs = transcript(SocketAddr::from(([127, 0, 0, 1], vanilla.port))).await;
    drop(vanilla);

    let hooks = MockHooks::new();
    {
        let mut cfg = default_configuration();
        cfg.registries = Arc::new(SomeRegistries);
        *hooks.configuration.lock() = Arc::new(cfg);
    }
    let server = TestServer::start(hooks);
    let ours = transcript(server.addr).await;
    for (a, b) in theirs.iter().zip(&ours) {
        assert_eq!(a, b, "vanilla (left) vs ours (right)");
    }
    assert_eq!(theirs.len(), ours.len());
    println!("{}", theirs.join("\n"));
}

// ---- our real server binary (registries, tags, packs) vs vanilla ------------------------------------------------------

/// Offline login to the point where the registries and tags were sent (hex of every `registry_data`/`update_tags` and of
/// the preceding packets).
async fn registry_transcript(addr: SocketAddr) -> Vec<String> {
    let mut t = Vec::new();
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let _ = read(&mut c).await; // compression
    c.set_compression(256, true);
    let _ = read(&mut c).await; // login finished
    write_payload(
        &mut c,
        &login_bytes(ServerboundLoginPacket::LoginAcknowledged(
            ServerboundLoginAcknowledgedPacket,
        )),
    )
    .await;
    loop {
        let p = read(&mut c).await;
        let name = name_of(ConnectionState::Configuration, &p);
        t.push(format!("{name} {}", hex(&p)));
        match name.as_str() {
            "minecraft:select_known_packs" => {
                let ClientboundConfigurationPacket::SelectKnownPacks(k) =
                    ClientboundConfigurationPacket::decode_payload(&p).unwrap()
                else {
                    panic!()
                };
                write_payload(
                    &mut c,
                    &config_bytes(ServerboundConfigurationPacket::ClientInformation(
                        ServerboundClientInformationPacket {
                            information: ClientInformation::default(),
                        },
                    )),
                )
                .await;
                write_payload(
                    &mut c,
                    &config_bytes(ServerboundConfigurationPacket::SelectKnownPacks(
                        ServerboundSelectKnownPacks {
                            known_packs: k.known_packs,
                        },
                    )),
                )
                .await;
            }
            "minecraft:update_tags" => return t,
            _ => {}
        }
    }
}

fn normalized_tags(line: &str) -> Vec<(String, Vec<(String, Vec<i32>)>)> {
    let bytes: Vec<u8> = (0..line.split(' ').nth(1).unwrap().len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&line.split(' ').nth(1).unwrap()[i..i + 2], 16).unwrap())
        .collect();
    let ClientboundConfigurationPacket::UpdateTags(p) =
        ClientboundConfigurationPacket::decode_payload(&bytes).unwrap()
    else {
        panic!()
    };
    let mut out: Vec<_> = p
        .tags
        .into_iter()
        .map(|(k, v)| {
            let mut tags: Vec<_> = v
                .tags
                .into_iter()
                .map(|(id, ids)| (id.to_string(), ids))
                .collect();
            tags.sort();
            (k.to_string(), tags)
        })
        .collect();
    out.sort();
    out
}

/// Each registry's tags are in `HashMap<Identifier, _>` bucket order (vanilla: oracle-verified, ties inside a bucket are
/// random per run; `mc_core::java_collections`): re-ordering the packet's order must be the identity.
fn assert_hash_map_tag_order(who: &str, line: &str) {
    let hex = line.split(' ').nth(1).unwrap();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let ClientboundConfigurationPacket::UpdateTags(p) =
        ClientboundConfigurationPacket::decode_payload(&bytes).unwrap()
    else {
        panic!()
    };
    for (registry, payload) in p.tags {
        let hashes: Vec<i32> = payload
            .tags
            .iter()
            .map(|(id, _)| id.java_hash_code())
            .collect();
        let order = mc_core::java_collections::hash_map_order(
            &hashes,
            mc_core::java_collections::HASH_MAP_DEFAULT_CAPACITY,
        );
        assert!(
            order.iter().enumerate().all(|(i, &j)| i == j),
            "{who}: tags of {registry} are not in HashMap bucket order"
        );
    }
}

/// Same `server.properties` for vanilla and our `mc-server` binary (the launcher sets port and bind address).
const PROPERTIES: [&str; 11] = [
    "online-mode=false",
    "white-list=false",
    "enforce-whitelist=false",
    "generate-structures=false",
    "spawn-protection=0",
    "max-tick-time=-1",
    "view-distance=2",
    "simulation-distance=2",
    "enable-rcon=false",
    "enable-query=false",
    "rate-limit=0",
];

/// `cargo test -p mc-server --test oracle real_server -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn real_server_registry_and_tag_packets_equal_vanilla() {
    let vanilla = mc_testkit::Server::vanilla()
        .properties(PROPERTIES)
        .start()
        .expect("start vanilla");
    let theirs = registry_transcript(SocketAddr::from(([127, 0, 0, 1], vanilla.port))).await;
    drop(vanilla);
    let ours_server = mc_testkit::Server::ours(env!("CARGO_BIN_EXE_mc-server"))
        .properties(PROPERTIES)
        .start()
        .expect("start our server");
    let ours = registry_transcript(SocketAddr::from(([127, 0, 0, 1], ours_server.port))).await;
    println!(
        "vanilla: {} packets, ours: {} packets",
        theirs.len(),
        ours.len()
    );
    for (i, (a, b)) in theirs.iter().zip(&ours).enumerate() {
        let name = a.split(' ').next().unwrap();
        if name == "minecraft:update_tags" {
            // Vanilla sends Java `HashMap` order (registries and tags); only the content is comparable.
            assert_eq!(
                normalized_tags(a),
                normalized_tags(b),
                "update_tags content differs: vanilla (left) vs ours (right)"
            );
            assert_hash_map_tag_order("vanilla", a);
            assert_hash_map_tag_order("ours", b);
        } else {
            assert_eq!(
                a, b,
                "packet #{i} ({name}) differs: vanilla (left) vs ours (right)"
            );
        }
    }
    assert_eq!(theirs.len(), ours.len());
}
