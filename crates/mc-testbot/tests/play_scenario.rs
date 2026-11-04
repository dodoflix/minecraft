//! Differential play scenario: the same scripted client (join, idle, legit movement incl. jump/sprint, speed hack,
//! teleport confirm, chat, commands, keep-alive) against the real vanilla server and against OUR `mc-server`
//! (`MC_SERVER_BIN`). Packet name sequences and server logs are written to `/tmp/scenario_{vanilla,ours}.{pkts,log}`
//! for comparison (`MC_SCENARIO=vanilla|ours`).
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mc_protocol::codec::BitSet;
use mc_protocol::packets::play::*;
use mc_testbot::conn::Conn;
use mc_testbot::join::*;
use mc_testbot::log::PacketLog;

struct Script {
    pos: (f64, f64, f64),
    on_ground: bool,
    tp_pending: Vec<ClientboundPlayerPositionPacket>,
    out: String,
}

struct W(mc_protocol::net::PacketWriter<tokio::net::tcp::OwnedWriteHalf>);

async fn send(conn: &mut W, p: ServerboundPlayPacket) {
    let mut buf = Vec::new();
    p.encode_payload(&mut buf);
    if conn.0.write_payload(&buf).await.is_err() || conn.0.flush().await.is_err() {
        eprintln!("send failed: connection closed");
    }
}

async fn pos(conn: &mut W, s: &mut Script, x: f64, y: f64, z: f64, ground: bool) {
    s.pos = (x, y, z);
    s.on_ground = ground;
    send(conn, ServerboundPlayPacket::MovePlayerPos(ServerboundMovePlayerPosPacket { x, y, z, on_ground: ground, horizontal_collision: false })).await;
}

/// Reads and answers packets for `dur`, one 50 ms client tick at a time; `tick` is called every tick.
async fn pump(conn: &mut W, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>, s: &mut Script, dur: Duration, mut tick: impl FnMut(u32) -> Option<(f64, f64, f64, bool)>) {
    let end = Instant::now() + dur;
    let mut n = 0u32;
    while Instant::now() < end {
        let tick_end = Instant::now() + Duration::from_millis(50);
        while Instant::now() < tick_end {
            let left = tick_end.saturating_duration_since(Instant::now());
            let Ok(Some(payload)) = tokio::time::timeout(left, rx.recv()).await else { break };
            let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap();
            let name = mc_protocol::packet::packet_name(mc_protocol::packet::ConnectionState::Play, mc_protocol::packet::PacketFlow::Clientbound, id).unwrap_or("?");
            match ClientboundPlayPacket::decode_payload(&payload) {
                Ok(p) => {
                    let dbg = format!("{p:?}");
                    let keep = !matches!(name, "minecraft:level_chunk_with_light" | "minecraft:update_tags" | "minecraft:update_recipes" | "minecraft:commands" | "minecraft:update_advancements" | "minecraft:set_entity_data" | "minecraft:update_attributes");
                    let _ = std::fs::write("/tmp/scenario_partial.pkts", &s.out);
                    s.out.push_str(&format!("{name} {}\n", if keep { dbg.chars().take(160).collect::<String>() } else { String::new() }));
                    match p {
                        ClientboundPlayPacket::KeepAlive(k) => send(conn, ServerboundPlayPacket::KeepAlive(ServerboundKeepAlivePacket { id: k.id })).await,
                        ClientboundPlayPacket::ChunkBatchFinished(_) => send(conn, ServerboundPlayPacket::ChunkBatchReceived(ServerboundChunkBatchReceivedPacket { desired_chunks_per_tick: 9.0 })).await,
                        ClientboundPlayPacket::PlayerPosition(tp) => {
                            send(conn, ServerboundPlayPacket::AcceptTeleportation(ServerboundAcceptTeleportationPacket { id: tp.id, x: tp.change.position.x, y: tp.change.position.y, z: tp.change.position.z, y_rot: tp.change.y_rot, x_rot: tp.change.x_rot })).await;
                            s.pos = (tp.change.position.x, tp.change.position.y, tp.change.position.z);
                            let (x, y, z) = s.pos;
                            send(conn, ServerboundPlayPacket::MovePlayerPosRot(ServerboundMovePlayerPosRotPacket { x, y, z, y_rot: tp.change.y_rot, x_rot: tp.change.x_rot, on_ground: false, horizontal_collision: false })).await;
                            send(conn, ServerboundPlayPacket::ClientTickEnd(ServerboundClientTickEndPacket)).await;
                            s.tp_pending.push(tp);
                        }
                        _ => {}
                    }
                }
                Err(e) => s.out.push_str(&format!("{name} <decode error {e}>\n")),
            }
        }
        if let Some((x, y, z, g)) = tick(n) {
            pos(conn, s, x, y, z, g).await;
        }
        send(conn, ServerboundPlayPacket::ClientTickEnd(ServerboundClientTickEndPacket)).await;
        n += 1;
    }
}

async fn scenario(port: u16, op: impl Fn(&str)) -> String {
    let opts = BotOptions::new("127.0.0.1", port, "E2EBot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log).await.unwrap();
    login_and_configure(&mut conn, &opts).await.expect("configure");
    let Conn { mut reader, writer, .. } = conn;
    let mut conn = W(writer);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok(p) = reader.read_payload().await {
            if tx.send(p).is_err() { break; }
        }
    });
    let mut s = Script { pos: (0.0, 0.0, 0.0), on_ground: true, tp_pending: vec![], out: String::new() };
    // join: wait for the first position (spawn), then load
    pump(&mut conn, &mut rx, &mut s, Duration::from_secs(3), |_| None).await;
    send(&mut conn, ServerboundPlayPacket::PlayerLoaded(ServerboundPlayerLoadedPacket)).await;
    { s.out.push_str("## idle 8 s\n"); eprintln!("## idle 8 s"); std::fs::write("/tmp/scenario_partial.pkts", &s.out).unwrap(); }
    let (x0, y0, z0) = s.pos;
    pump(&mut conn, &mut rx, &mut s, Duration::from_secs(8), |n| (n % 20 == 0).then_some((x0, y0, z0, true))).await;
    { s.out.push_str("## walk 20 ticks\n"); eprintln!("## walk 20 ticks"); std::fs::write("/tmp/scenario_partial.pkts", &s.out).unwrap(); }
    pump(&mut conn, &mut rx, &mut s, Duration::from_millis(1000), |n| Some((x0, y0, z0 + 0.2158 * f64::from(n + 1), true))).await;
    let (x1, y1, z1) = s.pos;
    { s.out.push_str("## jump\n"); eprintln!("## jump"); std::fs::write("/tmp/scenario_partial.pkts", &s.out).unwrap(); }
    let mut vy = 0.42f64;
    let mut y = y1;
    pump(&mut conn, &mut rx, &mut s, Duration::from_millis(1500), |n| {
        if n >= 12 + 8 && y <= y1 { return None; }
        y += vy;
        vy = (vy - 0.08) * 0.98;
        let g = y <= y1;
        if g { y = y1; }
        Some((x1, y, z1 + 0.2806 * f64::from(n + 1), g))
    }).await;
    let (x2, y2, z2) = s.pos;
    { s.out.push_str("## speed hack\n"); eprintln!("## speed hack"); std::fs::write("/tmp/scenario_partial.pkts", &s.out).unwrap(); }
    pump(&mut conn, &mut rx, &mut s, Duration::from_millis(100), |n| (n == 0).then_some((x2 + 500.0, y2, z2, true))).await;
    pump(&mut conn, &mut rx, &mut s, Duration::from_millis(1500), |_| None).await;
    { s.out.push_str("## chat+commands\n"); eprintln!("## chat+commands"); std::fs::write("/tmp/scenario_partial.pkts", &s.out).unwrap(); }
    send(&mut conn, ServerboundPlayPacket::Chat(ServerboundChatPacket { message: "hello".into(), time_stamp: mc_protocol::codec::Instant(0), salt: 0, signature: None, last_seen_messages: LastSeenMessagesUpdate { offset: 0, acknowledged: Default::default(), checksum: 0 } })).await;
    pump(&mut conn, &mut rx, &mut s, Duration::from_millis(500), |_| None).await;
    op("op E2EBot");
    pump(&mut conn, &mut rx, &mut s, Duration::from_millis(1000), |_| None).await;
    for c in ["time query daytime", "gamemode creative", "tp @s ~ ~5 ~", "say hi"] {
        send(&mut conn, ServerboundPlayPacket::ChatCommand(ServerboundChatCommandPacket { command: c.into() })).await;
        s.out.push_str(&format!("## cmd {c}\n"));
        pump(&mut conn, &mut rx, &mut s, Duration::from_millis(1000), |_| None).await;
    }
    { s.out.push_str("## keepalive wait 18 s\n"); eprintln!("## keepalive wait 18 s"); std::fs::write("/tmp/scenario_partial.pkts", &s.out).unwrap(); }
    pump(&mut conn, &mut rx, &mut s, Duration::from_secs(18), |_| None).await;
    let _ = BitSet::default();
    s.out
}

#[tokio::test(flavor = "multi_thread")]
async fn play_scenario() {
    let which = std::env::var("MC_SCENARIO").unwrap_or_default();
    let props = "online-mode=false\nwhite-list=false\nenforce-secure-profile=false\ngenerate-structures=false\nspawn-protection=0\nmax-tick-time=-1\nview-distance=4\nsimulation-distance=4\nenable-rcon=false\nenable-query=false\nrate-limit=0\nlevel-seed=12345\n";
    if which == "vanilla" {
        use mc_testbot::harness::VanillaServer;
        let server = VanillaServer::start("tb-scenario", 12345, "survival", &["enforce-secure-profile=false", "spawn-protection=0", "generate-structures=false"]).expect("vanilla");
        let out = scenario(server.port, |c| server.cmd(c)).await;
        std::fs::write("/tmp/scenario_vanilla.pkts", out).unwrap();
        std::fs::write("/tmp/scenario_vanilla.log", server.log()).unwrap();
        server.stop();
    } else if which == "ours" {
        let bin = std::env::var("MC_SERVER_BIN").expect("MC_SERVER_BIN");
        let port = mc_testbot::harness::free_port();
        let dir = std::env::temp_dir().join(format!("mc-scn-{}-{port}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("eula.txt"), "eula=true\n").unwrap();
        std::fs::write(dir.join("server.properties"), format!("{props}server-port={port}\nserver-ip=127.0.0.1\n")).unwrap();
        let mut child = Command::new(bin).arg("nogui").current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let log = Arc::new(Mutex::new(String::new()));
        let (tx, rx) = std::sync::mpsc::channel();
        let l2 = log.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                l2.lock().unwrap().push_str(&format!("{line}\n"));
                if line.contains("Done (") { let _ = tx.send(()); }
            }
        });
        rx.recv_timeout(Duration::from_secs(240)).expect("start");
        tokio::time::sleep(Duration::from_secs(1)).await;
        let stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));
        let out = scenario(port, |c| { let _ = writeln!(stdin.lock().unwrap(), "{c}"); }).await;
        std::fs::write("/tmp/scenario_ours.pkts", out).unwrap();
        std::fs::write("/tmp/scenario_ours.log", log.lock().unwrap().clone()).unwrap();
        let _ = child.kill();
    }
}
