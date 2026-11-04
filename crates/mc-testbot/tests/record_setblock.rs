//! Records, from the REAL vanilla server, the block-change packets a scripted `/setblock` + `/fill` sequence produces
//! (fixture `tests/fixtures/setblock_vanilla.txt`, compared with our level mutation by
//! `mc-game/tests/setblock_differential.rs`). Run with `MC_RECORD_VANILLA=1 cargo test -p mc-testbot --test record_setblock`.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_protocol::codec::{Decode, read_varint};
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::play::{ClientboundBlockEntityDataPacket, ClientboundBlockUpdatePacket, ClientboundLevelEventPacket, ClientboundSectionBlocksUpdatePacket};
use mc_testbot::harness::{VanillaServer, vanilla_available};
use mc_testbot::join::*;
use mc_testbot::log::PacketLog;

/// The scripted commands (also replayed by the game test).
pub const SCRIPT: &[&str] = &[
    "setblock 0 200 0 minecraft:stone",
    "setblock 0 200 0 minecraft:stone",
    "setblock 0 200 0 minecraft:dirt",
    "setblock 0 200 0 minecraft:glass destroy",
    "setblock 0 200 0 minecraft:air destroy",
    "setblock 0 200 0 minecraft:oak_stairs[facing=east,half=top] keep",
    "setblock 0 200 0 minecraft:stone keep",
    "setblock 1 200 1 minecraft:oak_stairs[facing=east,half=top]",
    "fill 0 201 0 3 201 3 minecraft:stone",
    "fill 0 201 0 3 201 3 minecraft:glass replace minecraft:stone",
    "fill 0 201 0 3 201 3 minecraft:air destroy",
    "fill 0 202 0 2 203 2 minecraft:gold_block hollow",
    "setblock 20 200 20 minecraft:stone",
];

#[tokio::test(flavor = "multi_thread")]
async fn record_vanilla_setblock() {
    if std::env::var("MC_RECORD_VANILLA").is_err() || !vanilla_available() {
        return;
    }
    let server = VanillaServer::start("tb-setblock", 12345, "survival", &[]).expect("start vanilla");
    let opts = BotOptions::new("127.0.0.1", server.port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log.clone()).await.unwrap();
    login_and_configure(&mut conn, &opts).await.expect("configure");
    drain(&mut conn, Duration::from_secs(8)).await;
    server.cmd("gamemode spectator Bot");
    server.cmd("tp Bot 0.5 205 0.5");
    drain(&mut conn, Duration::from_secs(3)).await;
    let mut out = String::new();
    for (i, command) in SCRIPT.iter().enumerate() {
        server.cmd(command);
        let packets = drain(&mut conn, Duration::from_millis(1200)).await;
        out.push_str(&format!("# {i} {command}\n"));
        for p in packets {
            out.push_str(&p);
            out.push('\n');
        }
    }
    std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/setblock_vanilla.txt"), out).unwrap();
}

async fn drain(conn: &mut Conn, duration: Duration) -> Vec<String> {
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + duration;
    while tokio::time::Instant::now() < deadline {
        let Ok(Ok(payload)) = tokio::time::timeout(Duration::from_millis(100), conn.reader.read_payload()).await else { continue };
        let mut body = &payload[..];
        let id = read_varint(&mut body).unwrap();
        let name = packet_name(ConnectionState::Play, PacketFlow::Clientbound, id).unwrap_or("?");
        let line = match name {
            "block_update" => ClientboundBlockUpdatePacket::decode(&mut body).map(|p| format!("{p:?}")),
            "section_blocks_update" => ClientboundSectionBlocksUpdatePacket::decode(&mut body).map(|p| format!("{p:?}")),
            "level_event" => ClientboundLevelEventPacket::decode(&mut body).map(|p| format!("{p:?}")),
            "block_entity_data" => ClientboundBlockEntityDataPacket::decode(&mut body).map(|p| format!("{p:?}")),
            _ => continue,
        };
        // the bot's own explosion/other level events (e.g. spectator teleport) are not part of the script
        lines.push(line.unwrap_or_else(|e| format!("undecodable {name}: {e}")));
    }
    lines
}
