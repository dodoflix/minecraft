//! Records, from the REAL vanilla server, the block-change packets a scripted `/setblock` + `/fill` sequence produces
//! (fixture `mc-game-core/tests/fixtures/setblock_vanilla.txt`, compared with our level mutation by
//! `mc-game-core/tests/golden/setblock_differential.rs`). Compares with the committed file; `MC_UPDATE_FIXTURES=1` rewrites it:
//! `MC_UPDATE_FIXTURES=1 cargo test -p mc-server --test oracle record_vanilla_setblock -- --ignored`.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_protocol::codec::{Decode, read_varint};
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::play::{ClientboundBlockEntityDataPacket, ClientboundBlockUpdatePacket, ClientboundLevelEventPacket, ClientboundSectionBlocksUpdatePacket};
use mc_testbot::conn::Conn;
use mc_testbot::join::*;
use mc_testbot::log::PacketLog;

/// The scripted commands (also replayed by the game test).
pub const SCRIPT: &[&str] = &[
    "setblock 90 140 -24 minecraft:stone",
    "setblock 90 140 -24 minecraft:stone",
    "setblock 90 140 -24 minecraft:dirt",
    "setblock 90 140 -24 minecraft:glass destroy",
    "setblock 90 140 -24 minecraft:air destroy",
    "setblock 90 140 -24 minecraft:oak_stairs[facing=east,half=top] keep",
    "setblock 90 140 -24 minecraft:stone keep",
    "setblock 91 140 -23 minecraft:oak_stairs[facing=east,half=top]",
    "fill 90 141 -24 93 141 -21 minecraft:stone",
    "fill 90 141 -24 93 141 -21 minecraft:glass replace minecraft:stone",
    "fill 90 141 -24 93 141 -21 minecraft:air destroy",
    "fill 90 142 -24 92 143 -22 minecraft:gold_block hollow",
    "setblock 110 140 -4 minecraft:stone",
];

/// Random ticks of the real world produce unrelated block updates elsewhere: keep only what happens in the script area
/// (blocks x 88..=112, y >= 139, z -26..=-2; sections 5..=6 x -2..=-1 at section y 8).
fn in_script_area(line: &str) -> bool {
    let num = |key: &str| -> Option<i32> {
        let rest = &line[line.find(key)? + key.len()..];
        let end = rest.find(|c: char| c != '-' && !c.is_ascii_digit())?;
        rest[..end].parse().ok()
    };
    if line.contains("SectionPos{") {
        return num("SectionPos{x=").is_some_and(|x| (5..=6).contains(&x)) && num("y=") == Some(8);
    }
    match (num("BlockPos{x="), num(", y="), num(", z=")) {
        (Some(x), Some(y), Some(z)) => (88..=112).contains(&x) && y >= 139 && (-26..=-2).contains(&z),
        _ => true,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn record_vanilla_setblock() {
    let server = mc_testbot::harness::vanilla(12345, "survival", &[]).start().expect("start vanilla");
    let opts = BotOptions::new("127.0.0.1", server.port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log.clone()).await.unwrap();
    login_and_configure(&mut conn, &opts).await.expect("configure");
    drain(&mut conn, Duration::from_secs(8)).await;
    server.cmd("gamemode spectator Bot");
    drain(&mut conn, Duration::from_secs(2)).await;
    let mut out = String::new();
    for (i, command) in SCRIPT.iter().enumerate() {
        server.cmd(command);
        let packets = drain(&mut conn, Duration::from_millis(1200)).await;
        out.push_str(&format!("# {i} {command}\n"));
        for p in packets.into_iter().filter(|p| std::env::var("MC_RECORD_ALL").is_ok() || in_script_area(p)) {
            out.push_str(&p);
            out.push('\n');
        }
    }
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-game-core/tests/fixtures/setblock_vanilla.txt");
    if std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1") {
        std::fs::write(&fixture, out).unwrap();
    } else {
        assert_eq!(std::fs::read_to_string(&fixture).expect("setblock_vanilla.txt"), out, "vanilla changed: rerun with MC_UPDATE_FIXTURES=1 to rewrite the fixture");
    }
}

async fn drain(conn: &mut Conn, duration: Duration) -> Vec<String> {
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + duration;
    while tokio::time::Instant::now() < deadline {
        // a single timeout per drain: `read_payload` is not cancellation safe, so it is only cancelled at the end
        let payload = match tokio::time::timeout_at(deadline, conn.reader.read_payload()).await {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                eprintln!("read error: {e}");
                break;
            }
            Err(_) => break,
        };
        let mut body = &payload[..];
        let id = read_varint(&mut body).unwrap();
        let name = packet_name(ConnectionState::Play, PacketFlow::Clientbound, id).unwrap_or("?");
        if std::env::var("MC_RECORD_DEBUG").is_ok() {
            eprintln!("packet {name}");
        }
        let line = match name {
            "minecraft:block_update" => ClientboundBlockUpdatePacket::decode(&mut body).map(|p| format!("{p:?}")),
            "minecraft:section_blocks_update" => ClientboundSectionBlocksUpdatePacket::decode(&mut body).map(|p| format!("{p:?}")),
            "minecraft:level_event" => ClientboundLevelEventPacket::decode(&mut body).map(|p| format!("{p:?}")),
            "minecraft:block_entity_data" => ClientboundBlockEntityDataPacket::decode(&mut body).map(|p| format!("{p:?}")),
            _ => continue,
        };
        // the bot's own explosion/other level events (e.g. spectator teleport) are not part of the script
        lines.push(line.unwrap_or_else(|e| format!("undecodable {name}: {e}")));
    }
    lines
}
