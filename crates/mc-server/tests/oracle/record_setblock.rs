//! Records, from the REAL vanilla server, the block-change packets a scripted `/setblock` + `/fill` sequence produces
//! (fixture `mc-game-core/tests/fixtures/setblock_vanilla.txt`, compared with our level mutation by
//! `mc-game-core/tests/golden/setblock_differential.rs`). Compares with the committed file; `MC_UPDATE_FIXTURES=1` rewrites it:
//! `MC_UPDATE_FIXTURES=1 cargo test -p mc-server --test oracle record_vanilla_setblock -- --ignored`.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_core::direction::Direction;
use mc_core::pos::block_pos::BlockPos;
use mc_protocol::codec::{Decode, read_varint};
use mc_protocol::packet::{ConnectionState, PacketFlow, packet_name};
use mc_protocol::packets::play::{
    ClientboundBlockEntityDataPacket, ClientboundBlockUpdatePacket, ClientboundDisconnectPacket,
    ClientboundLevelEventPacket, ClientboundSectionBlocksUpdatePacket,
    ServerboundChunkBatchReceivedPacket, ServerboundPlayPacket, ServerboundPlayerActionAction,
    ServerboundPlayerActionPacket, ServerboundPlayerLoadedPacket,
};
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
    // leaves of the fresh world update their distance while it settles; the script never touches leaves
    if line.contains("_leaves[") {
        return false;
    }
    if line.contains("SectionPos{") {
        return num("SectionPos{x=").is_some_and(|x| (5..=6).contains(&x))
            && num("y=") == Some(8)
            && num(", z=").is_some_and(|z| (-2..=-1).contains(&z));
    }
    match (num("BlockPos{x="), num(", y="), num(", z=")) {
        (Some(x), Some(y), Some(z)) => {
            (88..=112).contains(&x) && y >= 139 && (-26..=-2).contains(&z)
        }
        _ => true,
    }
}

/// Chunks (x, z) holding the script: the recording starts once the client has them, because the server sends no block
/// change for a chunk the client has not received yet.
const SCRIPT_CHUNKS: [(i32, i32); 2] = [(5, -2), (6, -1)];
const WARM_UP_FENCES: usize = 10;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn record_vanilla_setblock() {
    let server = mc_testbot::harness::vanilla(12345, "survival", &[])
        .start()
        .expect("start vanilla");
    let opts = BotOptions::new("127.0.0.1", server.port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log.clone()).await.unwrap();
    login_and_configure(&mut conn, &opts)
        .await
        .expect("configure");
    let mut received = Vec::new();
    read_until(&mut conn, |name, body| {
        if name == "minecraft:level_chunk_with_light" && body.len() >= 8 {
            let at = |i: usize| i32::from_be_bytes(body[i..i + 4].try_into().unwrap());
            received.push((at(0), at(4)));
        }
        SCRIPT_CHUNKS.iter().all(|c| received.contains(c))
    })
    .await;
    conn.send(&ServerboundPlayPacket::PlayerLoaded(
        ServerboundPlayerLoadedPacket,
    ))
    .await
    .expect("player loaded");
    server.cmd("gamemode spectator Bot");
    // the chunks only broadcast their block changes reliably some ticks after they were sent: let a few ticks pass
    for warm_up in 0..WARM_UP_FENCES {
        fence(&mut conn, &server, warm_up).await;
    }
    let mut out = String::new();
    for (i, command) in SCRIPT.iter().enumerate() {
        server.cmd(command);
        let packets = fence(&mut conn, &server, WARM_UP_FENCES + i).await;
        out.push_str(&format!("# {i} {command}\n"));
        for p in packets
            .into_iter()
            .filter(|p| std::env::var("MC_RECORD_ALL").is_ok() || in_script_area(p))
        {
            out.push_str(&p);
            out.push('\n');
        }
    }
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mc-game-core/tests/fixtures/setblock_vanilla.txt");
    if std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1") {
        std::fs::write(&fixture, out).unwrap();
    } else {
        let expected = std::fs::read_to_string(&fixture).expect("setblock_vanilla.txt");
        if expected != out {
            let (mut want, mut got) = (expected.lines(), out.lines());
            let first = loop {
                let (a, b) = (want.next(), got.next());
                if a != b || a.is_none() {
                    break format!("expected {a:?}\n   recorded {b:?}");
                }
            };
            panic!(
                "vanilla changed (rerun with MC_UPDATE_FIXTURES=1 to rewrite the fixture); first difference:\n{first}"
            );
        }
    }
}

/// The recorded packets of everything the server sent up to the end of the tick that broadcast the changes of the
/// console commands run before.
///
/// The dedicated server runs the packets queued by the clients at the start of a tick, then the levels (the chunk
/// broadcast sends the block changes), then ticks the connections (which send the acknowledgement of a block action)
/// and runs console input after that. The `say` line in the log says that every earlier console command ran; a block
/// action sent after it is read at the start of a later tick, so its acknowledgement follows the broadcast of those
/// commands' changes. (A reply to a chat command or a statistics request would come before the broadcast, and the
/// server kicks a client that sends more than a few chat commands per second.)
async fn fence(conn: &mut Conn, server: &mc_testkit::Server, n: usize) -> Vec<String> {
    server.cmd(&format!("say fence {n}"));
    server
        .wait_log(&format!(r"\[Server\] fence {n}$"), 120)
        .expect("the server logged the fence");
    conn.send(&ServerboundPlayPacket::PlayerAction(
        ServerboundPlayerActionPacket {
            action: ServerboundPlayerActionAction::AbortDestroyBlock,
            pos: BlockPos::new(0, 0, 0),
            direction: Direction::Down,
            sequence: n as i32,
        },
    ))
    .await
    .expect("send the block action");
    read_until(conn, |name, _| name == "minecraft:block_changed_ack").await
}

/// Reads packets until `stop(name, body)` returns true for one (that packet is not recorded) and returns the recorded
/// lines. Fails after two minutes.
async fn read_until(conn: &mut Conn, mut stop: impl FnMut(&str, &[u8]) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        // a single timeout for the whole loop: `read_payload` is not cancellation safe, so it only ends in the failure
        let payload = tokio::time::timeout_at(deadline, conn.reader.read_payload())
            .await
            .expect("the awaited packet never arrived")
            .expect("read a packet");
        let mut body = &payload[..];
        let id = read_varint(&mut body).unwrap();
        let name = packet_name(ConnectionState::Play, PacketFlow::Clientbound, id).unwrap_or("?");
        if std::env::var("MC_RECORD_DEBUG").is_ok() {
            eprintln!("packet {name}");
        }
        assert!(
            name != "minecraft:disconnect",
            "the server kicked the bot: {:?}",
            ClientboundDisconnectPacket::decode(&mut &body[..]).map(|p| format!("{p:?}"))
        );
        if stop(name, body) {
            return lines;
        }
        // Acknowledge every chunk batch like a real client: otherwise the server keeps the chunks past the first batch
        // pending, and which chunks make the first batch depends on how fast they generate (block changes of pending
        // chunks are not sent).
        if name == "minecraft:chunk_batch_finished" {
            conn.send(&ServerboundPlayPacket::ChunkBatchReceived(
                ServerboundChunkBatchReceivedPacket {
                    desired_chunks_per_tick: 9.0,
                },
            ))
            .await
            .expect("ack chunk batch");
            continue;
        }
        let line = match name {
            "minecraft:block_update" => {
                ClientboundBlockUpdatePacket::decode(&mut body).map(|p| format!("{p:?}"))
            }
            "minecraft:section_blocks_update" => {
                ClientboundSectionBlocksUpdatePacket::decode(&mut body).map(|p| format!("{p:?}"))
            }
            "minecraft:level_event" => {
                ClientboundLevelEventPacket::decode(&mut body).map(|p| format!("{p:?}"))
            }
            "minecraft:block_entity_data" => {
                ClientboundBlockEntityDataPacket::decode(&mut body).map(|p| format!("{p:?}"))
            }
            _ => continue,
        };
        // the bot's own explosion/other level events (e.g. spectator teleport) are not part of the script
        lines.push(line.unwrap_or_else(|e| format!("undecodable {name}: {e}")));
    }
}
