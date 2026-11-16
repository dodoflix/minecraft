//! Differential tests: the same client scenarios against the real vanilla server (`ref/server.jar`), against our
//! listener (mock hooks configured like a default offline vanilla server) and against our `mc-server` binary;
//! transcripts must be identical. Needs java, the jar and `MC_ACCEPT_EULA=1` (`docs/testing.md`); without them the
//! tests fail.
use crate::net_support::*;
use mc_game::server::network::hooks::RegistrySynchronization;
use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::codec::read_varint;
use mc_protocol::net::Connection;
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::common::{ClientInformation, ResourcePackAction};
use mc_protocol::packets::configuration::*;
use mc_protocol::packets::handshake::ClientIntent;
use mc_protocol::packets::login::*;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
        (PROTOCOL_VERSION, ClientIntent::Transfer),
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

    login_to_end_of_configuration(addr, ClientIntent::Login, &mut t).await;
    t
}

/// What the client does in the configuration phase besides answering the registry synchronisation.
#[derive(Clone, Copy)]
enum Config {
    /// Runs to `finish_configuration`.
    Finish,
    /// Answers the known packs request with a cookie response the server never asked for.
    UnexpectedCookie,
    /// Answers every resource pack push with this action.
    Pack(ResourcePackAction),
}

/// Offline login as `name` with `intent` up to the acknowledgement of the login; the compression and profile packets
/// are transcript lines.
async fn login_to_configuration(
    addr: SocketAddr,
    intent: ClientIntent,
    name: &str,
    t: &mut Vec<String>,
) -> Connection<TcpStream> {
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, intent).await;
    hello(&mut c, name).await;
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
    c
}

/// The configuration phase until `finish_configuration`, a disconnect or the close of the connection; every packet is
/// a transcript line (the registry data run is one line).
async fn configuration(c: &mut Connection<TcpStream>, t: &mut Vec<String>, config: Config) {
    let mut registry_run = false;
    loop {
        let Ok(Ok(p)) = tokio::time::timeout(Duration::from_secs(120), c.read_payload()).await
        else {
            t.push("config closed".into());
            return;
        };
        let name = name_of(ConnectionState::Configuration, &p);
        // A server that takes long to prepare the spawn sends keep-alives meanwhile; they are not part of the transcript.
        if let Ok(ClientboundConfigurationPacket::KeepAlive(k)) =
            ClientboundConfigurationPacket::decode_payload(&p)
        {
            write_payload(
                c,
                &config_bytes(ServerboundConfigurationPacket::KeepAlive(
                    ServerboundKeepAlivePacket { id: k.id },
                )),
            )
            .await;
            continue;
        }
        match name.as_str() {
            "minecraft:registry_data" => {
                if !registry_run {
                    t.push("config minecraft:registry_data*".into());
                    registry_run = true;
                }
                continue;
            }
            "minecraft:custom_payload"
            | "minecraft:disconnect"
            | "minecraft:resource_pack_push"
            | "minecraft:update_enabled_features"
            | "minecraft:select_known_packs" => t.push(format!("config {name} {}", hex(&p))),
            _ => t.push(format!("config {name}")),
        }
        match ClientboundConfigurationPacket::decode_payload(&p) {
            Ok(ClientboundConfigurationPacket::SelectKnownPacks(k)) => {
                write_payload(
                    c,
                    &config_bytes(ServerboundConfigurationPacket::ClientInformation(
                        ServerboundClientInformationPacket {
                            information: ClientInformation::default(),
                        },
                    )),
                )
                .await;
                let reply = match config {
                    Config::UnexpectedCookie => ServerboundConfigurationPacket::CookieResponse(
                        mc_protocol::packets::configuration::ServerboundCookieResponsePacket {
                            key: mc_core::ident::Identifier::with_default_namespace("x").unwrap(),
                            payload: None,
                        },
                    ),
                    _ => ServerboundConfigurationPacket::SelectKnownPacks(
                        ServerboundSelectKnownPacks {
                            known_packs: k.known_packs,
                        },
                    ),
                };
                write_payload(c, &config_bytes(reply)).await;
            }
            Ok(ClientboundConfigurationPacket::ResourcePackPush(push)) => {
                if let Config::Pack(action) = config {
                    write_payload(
                        c,
                        &config_bytes(ServerboundConfigurationPacket::ResourcePack(
                            ServerboundResourcePackPacket {
                                id: push.id,
                                action,
                            },
                        )),
                    )
                    .await;
                }
            }
            _ => {}
        }
        if name == "minecraft:finish_configuration" || name == "minecraft:disconnect" {
            return;
        }
    }
}

/// A whole login as `Steve` with `intent` up to `finish_configuration`.
async fn login_to_end_of_configuration(
    addr: SocketAddr,
    intent: ClientIntent,
    t: &mut Vec<String>,
) {
    let mut c = login_to_configuration(addr, intent, "Steve", t).await;
    configuration(&mut c, t, Config::Finish).await;
    assert!(
        t.last()
            .is_some_and(|l| l.starts_with("config minecraft:finish_configuration")),
        "configuration did not finish: {t:?}"
    );
}

/// Transfer intent with `accepts-transfers=true`, and a cookie response in the configuration phase.
async fn transfer_and_cookie(addr: SocketAddr) -> Vec<String> {
    let mut t = vec!["transfer".to_owned()];
    login_to_end_of_configuration(addr, ClientIntent::Transfer, &mut t).await;
    t.push("cookie in configuration".into());
    let mut c = login_to_configuration(addr, ClientIntent::Login, "Steve", &mut t).await;
    configuration(&mut c, &mut t, Config::UnexpectedCookie).await;
    t
}

/// A required resource pack: declined (kick) and loaded (configuration finishes).
async fn required_resource_pack(addr: SocketAddr) -> Vec<String> {
    let mut t = Vec::new();
    for action in [
        ResourcePackAction::Declined,
        ResourcePackAction::SuccessfullyLoaded,
    ] {
        t.push(format!("pack {action:?}"));
        let mut c = login_to_configuration(addr, ClientIntent::Login, "Steve", &mut t).await;
        configuration(&mut c, &mut t, Config::Pack(action)).await;
    }
    t
}

/// Hex of the one packet the server answers a status request with (or `closed`), and the players it reports online.
async fn status_reply(addr: SocketAddr) -> (String, Option<i32>) {
    use mc_protocol::packets::status::{
        ClientboundStatusPacket, ServerboundStatusPacket, ServerboundStatusRequestPacket,
    };
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Status).await;
    let mut request = Vec::new();
    ServerboundStatusPacket::StatusRequest(ServerboundStatusRequestPacket)
        .encode_payload(&mut request)
        .unwrap();
    write_payload(&mut c, &request).await;
    let reply = single_reply(&mut c).await;
    let online = (0..reply.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&reply[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .ok()
        .and_then(
            |bytes| match ClientboundStatusPacket::decode_payload(&bytes) {
                Ok(ClientboundStatusPacket::StatusResponse(s)) => {
                    s.status.players.map(|p| p.online)
                }
                _ => None,
            },
        );
    (reply, online)
}

/// Status before and after a login (`Steve` stays connected), then a second login: refused when the server is full.
/// The server caches its status for a few seconds, so the second status is the first that shows the player.
async fn population(addr: SocketAddr, full: bool) -> Vec<String> {
    let mut t = vec![format!("status empty {}", status_reply(addr).await.0)];
    let mut options = mc_testbot::join::BotOptions::new("127.0.0.1", addr.port(), "Steve");
    options.timeout = Duration::from_secs(120);
    let steve = mc_testbot::Bot::connect(options).await.expect("connect");
    steve
        .wait_joined(Duration::from_secs(60))
        .await
        .expect("login packet");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let shown = loop {
        let (reply, online) = status_reply(addr).await;
        if online == Some(1) || tokio::time::Instant::now() >= deadline {
            break reply;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    t.push(format!("status one {shown}"));
    if full {
        let mut c = connect(addr).await;
        handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
        hello(&mut c, "Alex").await;
        t.push(format!("login Alex {}", single_reply(&mut c).await));
    }
    steve.disconnect().await;
    t
}

/// The same scenario against the real vanilla server and `mc-server`, started with the same properties.
async fn same_on_both<F: AsyncFn(SocketAddr) -> Vec<String>>(properties: &[&str], scenario: F) {
    let vanilla = mc_testkit::Server::vanilla()
        .properties(PROPERTIES)
        .properties(properties.iter().copied())
        .start()
        .expect("start vanilla");
    let theirs = scenario(SocketAddr::from(([127, 0, 0, 1], vanilla.port))).await;
    drop(vanilla);
    let server = mc_testkit::Server::ours(env!("CARGO_BIN_EXE_mc-server"))
        .properties(PROPERTIES)
        .properties(properties.iter().copied())
        .start()
        .expect("start our server");
    let ours = scenario(SocketAddr::from(([127, 0, 0, 1], server.port))).await;
    println!("{}", theirs.join("\n"));
    assert_same("our server", &theirs, &ours);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn transfers_and_cookie_responses_equal_vanilla() {
    same_on_both(&["accepts-transfers=true"], transfer_and_cookie).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn required_resource_pack_equals_vanilla() {
    same_on_both(
        &[
            "resource-pack=http://127.0.0.1:9/pack.zip",
            "resource-pack-sha1=0123456789abcdef0123456789abcdef01234567",
            "resource-pack-id=0a0b0c0d-0e0f-1011-1213-141516171819",
            "resource-pack-prompt={\"text\":\"Use the pack\"}",
            "require-resource-pack=true",
        ],
        required_resource_pack,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn disabled_status_equals_vanilla() {
    same_on_both(&["enable-status=false"], async |addr| {
        let mut t = vec![format!("status {}", status_reply(addr).await.0)];
        login_to_end_of_configuration(addr, ClientIntent::Login, &mut t).await;
        t
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn full_server_and_status_sample_equal_vanilla() {
    same_on_both(
        &["max-players=1", "level-type=minecraft:flat"],
        async |addr| population(addr, true).await,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn hidden_online_players_equal_vanilla() {
    same_on_both(
        &[
            "hide-online-players=true",
            "max-players=3",
            "level-type=minecraft:flat",
        ],
        async |addr| population(addr, false).await,
    )
    .await;
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
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
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
    let mock = transcript(server.addr).await;
    assert_same("our listener", &theirs, &mock);

    let ours_server = mc_testkit::Server::ours(env!("CARGO_BIN_EXE_mc-server"))
        .properties(PROPERTIES)
        .start()
        .expect("start our server");
    let ours = transcript(SocketAddr::from(([127, 0, 0, 1], ours_server.port))).await;
    assert_same("our server", &theirs, &ours);
    println!("{}", theirs.join("\n"));
}

/// Line-by-line comparison with the vanilla transcript on the left.
fn assert_same(who: &str, vanilla: &[String], ours: &[String]) {
    for (i, (a, b)) in vanilla.iter().zip(ours).enumerate() {
        assert_eq!(a, b, "{who}, line {i}: vanilla (left) vs ours (right)");
    }
    assert_eq!(vanilla.len(), ours.len(), "{who}: transcript length");
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

type TagRegistries = Vec<(String, Vec<(String, Vec<i32>)>)>;

fn normalized_tags(line: &str) -> TagRegistries {
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
