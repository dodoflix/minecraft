//! Regenerates the vanilla registry fixtures of `golden::vanilla_oracle`.
//!
//! `regen_oracle_fixtures` starts the vanilla server on three worlds (default, custom data packs, broken data pack),
//! connects as a client that knows no packs (so `registry_data` carries every entry) and stores the raw configuration packets
//! and the server log under `tests/fixtures/oracle/`.
//! Regenerate: `MC_UPDATE_FIXTURES=1 cargo test -p mc-game --test oracle -- --ignored regen_oracle_fixtures --nocapture`.

use crate::test_packs;

use std::path::{Path, PathBuf};
use std::time::Duration;

use mc_protocol::PROTOCOL_VERSION;
use mc_protocol::net::Connection;
use mc_protocol::packets::{configuration::*, handshake::*, login::*};
use tokio::net::TcpStream;
use tokio::time::timeout;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/oracle")
}

struct Captured {
    /// Raw payloads of `registry_data` / `update_tags` packets in arrival order.
    packets: Vec<Vec<u8>>,
    log: Vec<String>,
}

/// Run the vanilla server in a scratch dir after `setup(world_dir)`; with `connect` also capture the configuration packets.
/// Without `connect` the server is expected to fail while starting (broken data pack).
async fn run_vanilla<F: FnOnce(&Path) + 'static>(setup: F, connect: bool) -> Captured {
    let builder = mc_testkit::Server::vanilla()
        .properties([
            "online-mode=false",
            "white-list=false",
            "enforce-whitelist=false",
            "enforce-secure-profile=false",
            "generate-structures=false",
            "spawn-protection=0",
            "max-tick-time=-1",
            "view-distance=2",
            "simulation-distance=2",
            "enable-rcon=false",
            "enable-query=false",
            "rate-limit=0",
        ])
        .prepare(move |dir| setup(&dir.join("world")));
    let mut packets = Vec::new();
    let server = if connect {
        let server = builder.start().expect("start vanilla");
        packets = capture_configuration(server.port).await;
        server
    } else {
        let server = builder.start_unready().expect("start vanilla");
        server.wait_exit(Duration::from_secs(60));
        server
    };
    server.kill();
    Captured {
        packets,
        log: server.log().lines().map(str::to_owned).collect(),
    }
}

macro_rules! send {
    ($conn:expr, $enum:ident :: $variant:ident($packet:expr)) => {{
        let mut buf = Vec::new();
        $enum::$variant($packet).encode_payload(&mut buf).unwrap();
        $conn.write_payload(&buf).await.unwrap();
        $conn.flush().await.unwrap();
    }};
}

async fn capture_configuration(port: u16) -> Vec<Vec<u8>> {
    let mut c = Connection::new(TcpStream::connect(("127.0.0.1", port)).await.unwrap());
    send!(
        c,
        ServerboundHandshakePacket::ClientIntention(ClientIntentionPacket {
            protocol_version: PROTOCOL_VERSION,
            host_name: "localhost".into(),
            port,
            intention: ClientIntent::Login
        })
    );
    send!(
        c,
        ServerboundLoginPacket::Hello(ServerboundHelloPacket {
            name: "OracleBot".into(),
            profile_id: uuid::Uuid::from_u128(1)
        })
    );
    loop {
        let payload = timeout(Duration::from_secs(30), c.read_payload())
            .await
            .unwrap()
            .unwrap();
        match ClientboundLoginPacket::decode_payload(&payload).unwrap() {
            ClientboundLoginPacket::LoginCompression(p) => {
                c.set_compression(p.compression_threshold, false)
            }
            ClientboundLoginPacket::LoginFinished(_) => break,
            other => panic!("unexpected {other:?}"),
        }
    }
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
    let mut out = Vec::new();
    loop {
        let payload = timeout(Duration::from_secs(30), c.read_payload())
            .await
            .unwrap()
            .unwrap();
        match ClientboundConfigurationPacket::decode_payload(&payload).unwrap() {
            ClientboundConfigurationPacket::SelectKnownPacks(_) => send!(
                c,
                ServerboundConfigurationPacket::SelectKnownPacks(ServerboundSelectKnownPacks {
                    known_packs: vec![]
                })
            ),
            ClientboundConfigurationPacket::KeepAlive(p) => send!(
                c,
                ServerboundConfigurationPacket::KeepAlive(ServerboundKeepAlivePacket { id: p.id })
            ),
            ClientboundConfigurationPacket::Ping(p) => send!(
                c,
                ServerboundConfigurationPacket::Pong(ServerboundPongPacket { id: p.id })
            ),
            ClientboundConfigurationPacket::FinishConfiguration(_) => break,
            ClientboundConfigurationPacket::RegistryData(_)
            | ClientboundConfigurationPacket::UpdateTags(_) => out.push(payload),
            _ => {}
        }
    }
    out
}

fn write_fixture(dir: &Path, name: &str, captured: &Captured, expect_packets: bool) {
    assert!(
        !expect_packets || !captured.packets.is_empty(),
        "{name}: no configuration packets captured from vanilla"
    );
    std::fs::create_dir_all(dir).unwrap();
    let mut bin = Vec::new();
    for p in &captured.packets {
        bin.extend((p.len() as u32).to_be_bytes());
        bin.extend(p);
    }
    std::fs::write(dir.join(format!("{name}.bin")), bin).unwrap();
    // `.log` is git-ignored: the server logs would silently be missing from a clone
    std::fs::write(
        dir.join(format!("{name}.server-log.txt")),
        captured.log.join("\n") + "\n",
    )
    .unwrap();
}

/// Records the three worlds into the committed fixtures. It is a generator, not a check: it does nothing unless
/// `MC_UPDATE_FIXTURES=1` is set, so the `--ignored` oracle run never rewrites tracked files.
#[test]
#[ignore = "oracle: starts the vanilla server (only with MC_UPDATE_FIXTURES=1)"]
fn regen_oracle_fixtures() {
    if !std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1") {
        return;
    }
    let dir = fixture_dir();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        write_fixture(&dir, "default", &run_vanilla(|_| {}, true).await, true);
        write_fixture(
            &dir,
            "custom",
            &run_vanilla(test_packs::write_custom_packs, true).await,
            true,
        );
        write_fixture(
            &dir,
            "broken",
            &run_vanilla(test_packs::write_broken_packs, false).await,
            false,
        );
    });
}
