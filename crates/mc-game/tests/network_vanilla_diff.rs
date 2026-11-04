//! Differential test: the same client scenarios against the real vanilla server (`ref/server.jar`) and against our
//! listener (mock hooks configured like a default offline vanilla server); transcripts must be identical. Skipped when
//! java or the jar is missing.
mod net_common;
use net_common as common;

use common::*;
use mc_game::server::network::hooks::RegistrySynchronization;
use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::codec::read_varint;
use mc_protocol::net::Connection;
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::common::ClientInformation;
use mc_protocol::packets::configuration::*;
use mc_protocol::packets::handshake::ClientIntent;
use mc_protocol::packets::login::*;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

struct Vanilla {
    child: Child,
    port: u16,
    dir: PathBuf,
}

impl Drop for Vanilla {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn start_vanilla() -> Option<Vanilla> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().ok()?;
    let jar = root.join("ref/server.jar");
    if !jar.exists() || Command::new("java").arg("-version").output().is_err() {
        eprintln!("skipping: java or ref/server.jar not available");
        return None;
    }
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("mc-game-net-diff-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("eula.txt"), "eula=true\n").ok()?;
    std::fs::write(
        dir.join("server.properties"),
        format!("online-mode=false\nwhite-list=false\nenforce-whitelist=false\nserver-port={port}\nserver-ip=127.0.0.1\ngenerate-structures=false\nspawn-protection=0\nmax-tick-time=-1\nview-distance=2\nsimulation-distance=2\nenable-rcon=false\nenable-query=false\nrate-limit=0\n"),
    )
    .ok()?;
    let link = dir.join("net-diff-vanilla.jar");
    std::os::unix::fs::symlink(&jar, &link).ok()?;
    let mut child = Command::new("java")
        .args(["-Xmx1G", "-jar"])
        .arg(&link)
        .arg("nogui")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut tx = Some(tx);
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("Done (") {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(());
                }
            }
        }
    });
    let vanilla = Vanilla { child, port, dir };
    rx.recv_timeout(Duration::from_secs(180)).expect("vanilla server did not finish starting");
    // The listener accepts at "Done" but the server thread still finishes startup work.
    std::thread::sleep(Duration::from_secs(3));
    Some(vanilla)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

fn name_of(state: ConnectionState, payload: &[u8]) -> String {
    let id = read_varint(&mut &payload[..]).unwrap();
    packet_name(state, PacketFlow::Clientbound, id).unwrap_or("?").to_owned()
}

fn login_bytes(p: ServerboundLoginPacket) -> Vec<u8> {
    let mut b = Vec::new();
    p.encode_payload(&mut b);
    b
}

fn config_bytes(p: ServerboundConfigurationPacket) -> Vec<u8> {
    let mut b = Vec::new();
    p.encode_payload(&mut b);
    b
}

async fn hello(c: &mut Connection<TcpStream>, name: &str) {
    write_payload(c, &login_bytes(ServerboundLoginPacket::Hello(ServerboundHelloPacket { name: name.into(), profile_id: Uuid::nil() }))).await;
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
    let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
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
    for (protocol, intent) in [(753, ClientIntent::Login), (754, ClientIntent::Login), (PROTOCOL_VERSION + 1, ClientIntent::Login), (10, ClientIntent::Transfer)] {
        let mut c = connect(addr).await;
        handshake(&mut c, protocol, intent).await;
        t.push(format!("handshake {protocol} {intent:?} {}", single_reply(&mut c).await));
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
    write_payload(&mut c, &login_bytes(ServerboundLoginPacket::LoginAcknowledged(ServerboundLoginAcknowledgedPacket))).await;
    t.push(format!("early ack {}", single_reply(&mut c).await));
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    write_payload(
        &mut c,
        &login_bytes(ServerboundLoginPacket::CookieResponse(mc_protocol::packets::login::ServerboundCookieResponsePacket {
            key: mc_core::ident::Identifier::with_default_namespace("x").unwrap(),
            payload: None,
        })),
    )
    .await;
    t.push(format!("cookie {}", single_reply(&mut c).await));
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    write_payload(&mut c, &login_bytes(ServerboundLoginPacket::Key(ServerboundKeyPacket { keybytes: vec![1], encrypted_challenge: vec![2] }))).await;
    t.push(format!("early key {}", single_reply(&mut c).await));

    // full offline flow up to the first play packet
    let mut c = connect(addr).await;
    handshake(&mut c, PROTOCOL_VERSION, ClientIntent::Login).await;
    hello(&mut c, "Steve").await;
    let p = read(&mut c).await;
    t.push(format!("login {}", hex(&p)));
    c.set_compression(256, true);
    let p = read(&mut c).await;
    let ClientboundLoginPacket::LoginFinished(f) = ClientboundLoginPacket::decode_payload(&p).unwrap() else { panic!("finished") };
    t.push(format!("finished {} {} {:?}", f.game_profile.id, f.game_profile.name, f.game_profile.properties));
    write_payload(&mut c, &login_bytes(ServerboundLoginPacket::LoginAcknowledged(ServerboundLoginAcknowledgedPacket))).await;
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
            "minecraft:custom_payload" | "minecraft:update_enabled_features" | "minecraft:select_known_packs" => {
                t.push(format!("config {name} {}", hex(&p)));
                if name == "minecraft:select_known_packs" {
                    let ClientboundConfigurationPacket::SelectKnownPacks(k) = ClientboundConfigurationPacket::decode_payload(&p).unwrap() else { panic!() };
                    let known = k.known_packs;
                    write_payload(
                        &mut c,
                        &config_bytes(ServerboundConfigurationPacket::ClientInformation(ServerboundClientInformationPacket {
                            information: ClientInformation::default(),
                        })),
                    )
                    .await;
                    write_payload(&mut c, &config_bytes(ServerboundConfigurationPacket::SelectKnownPacks(ServerboundSelectKnownPacks { known_packs: known }))).await;
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
    fn pack_registries(&self, _negotiated: &std::collections::HashSet<KnownPack>) -> Vec<ClientboundRegistryDataPacket> {
        (0..3)
            .map(|i| ClientboundRegistryDataPacket { registry: mc_core::ident::Identifier::with_default_namespace(&format!("r{i}")).unwrap(), entries: vec![] })
            .collect()
    }
    fn tags(&self) -> ClientboundUpdateTagsPacket {
        ClientboundUpdateTagsPacket { tags: vec![] }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn transcripts_equal_the_real_vanilla_server() {
    let Some(vanilla) = start_vanilla() else { return };
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
    write_payload(&mut c, &login_bytes(ServerboundLoginPacket::LoginAcknowledged(ServerboundLoginAcknowledgedPacket))).await;
    loop {
        let p = read(&mut c).await;
        let name = name_of(ConnectionState::Configuration, &p);
        t.push(format!("{name} {}", hex(&p)));
        match name.as_str() {
            "minecraft:select_known_packs" => {
                let ClientboundConfigurationPacket::SelectKnownPacks(k) = ClientboundConfigurationPacket::decode_payload(&p).unwrap() else { panic!() };
                write_payload(
                    &mut c,
                    &config_bytes(ServerboundConfigurationPacket::ClientInformation(ServerboundClientInformationPacket { information: ClientInformation::default() })),
                )
                .await;
                write_payload(&mut c, &config_bytes(ServerboundConfigurationPacket::SelectKnownPacks(ServerboundSelectKnownPacks { known_packs: k.known_packs }))).await;
            }
            "minecraft:update_tags" => return t,
            _ => {}
        }
    }
}

/// Runs `MC_SERVER_BIN` (our `mc-server` binary) in a scratch directory with the same properties as the vanilla run.
fn start_ours(bin: &str) -> Option<Vanilla> {
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("mc-game-net-ours-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("eula.txt"), "eula=true\n").ok()?;
    std::fs::write(
        dir.join("server.properties"),
        format!("online-mode=false\nwhite-list=false\nenforce-whitelist=false\nserver-port={port}\nserver-ip=127.0.0.1\ngenerate-structures=false\nspawn-protection=0\nmax-tick-time=-1\nview-distance=2\nsimulation-distance=2\nenable-rcon=false\nenable-query=false\nrate-limit=0\n"),
    )
    .ok()?;
    let mut child = Command::new(bin).arg("nogui").current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut tx = Some(tx);
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("Done (") {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(());
                }
            }
        }
    });
    let ours = Vanilla { child, port, dir };
    rx.recv_timeout(Duration::from_secs(180)).expect("our server did not finish starting");
    std::thread::sleep(Duration::from_secs(1));
    Some(ours)
}

/// `MC_SERVER_BIN=target/<dir>/debug/mc-server cargo test -p mc-game --test network_vanilla_diff real_server -- --nocapture`
#[tokio::test(flavor = "multi_thread")]
async fn real_server_registry_and_tag_packets_equal_vanilla() {
    let Ok(bin) = std::env::var("MC_SERVER_BIN") else {
        eprintln!("skipping: set MC_SERVER_BIN to our mc-server binary");
        return;
    };
    let Some(vanilla) = start_vanilla() else { return };
    let theirs = registry_transcript(SocketAddr::from(([127, 0, 0, 1], vanilla.port))).await;
    drop(vanilla);
    let ours_server = start_ours(&bin).expect("start our server");
    let ours = registry_transcript(SocketAddr::from(([127, 0, 0, 1], ours_server.port))).await;
    println!("vanilla: {} packets, ours: {} packets", theirs.len(), ours.len());
    for (i, (a, b)) in theirs.iter().zip(&ours).enumerate() {
        let name = a.split(' ').next().unwrap();
        assert_eq!(a, b, "packet #{i} ({name}) differs: vanilla (left) vs ours (right)");
    }
    assert_eq!(theirs.len(), ours.len());
}
