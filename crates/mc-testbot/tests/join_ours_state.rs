//! Joins OUR server on a world seeded with scoreboard / custom boss event / world border saved data (copied from
//! `MC_SEED_WORLD_DIR`, the dir that contains `world/`) and checks the matching join packets arrive.
//! Needs `MC_SERVER_BIN` (path of `mc-server`) and `MC_SEED_WORLD_DIR`; skipped otherwise.
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_testbot::join::*;
use mc_testbot::log::PacketLog;

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn join_with_scoreboard_boss_bar_and_border() {
    let (Ok(bin), Ok(seed)) = (std::env::var("MC_SERVER_BIN"), std::env::var("MC_SEED_WORLD_DIR")) else { return };
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("mc-join-state-{}-{port}", std::process::id()));
    copy_dir(std::path::Path::new(&seed), &dir);
    std::fs::write(dir.join("eula.txt"), "eula=true\n").unwrap();
    std::fs::write(
        dir.join("server.properties"),
        format!("online-mode=false\nwhite-list=false\nserver-port={port}\nserver-ip=127.0.0.1\ngenerate-structures=false\nspawn-protection=0\nmax-tick-time=-1\nview-distance=2\nsimulation-distance=2\nenable-rcon=false\nenable-query=false\nrate-limit=0\nlevel-seed=12345\n"),
    )
    .unwrap();
    let mut child = Command::new(bin).arg("nogui").current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut all = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
            let _ = std::fs::write("/tmp/ours_state_server.log", &all);
            if line.contains("Done (") {
                let _ = tx.send(());
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(240)).expect("server start");
    tokio::time::sleep(Duration::from_secs(1)).await;

    let opts = BotOptions::new("127.0.0.1", port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log.clone()).await.unwrap();
    login_and_configure(&mut conn, &opts).await.expect("configuration");
    let mut names: Vec<String> = Vec::new();
    let mut details = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        let Ok(Ok(payload)) = tokio::time::timeout(Duration::from_millis(500), conn.reader.read_payload()).await else { continue };
        let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap();
        let name = mc_protocol::packet::packet_name(mc_protocol::packet::ConnectionState::Play, mc_protocol::packet::PacketFlow::Clientbound, id).unwrap_or("?");
        details.push_str(&format!("{name} {}\n", payload.iter().take(60).map(|b| format!("{b:02x}")).collect::<String>()));
        names.push(name.trim_start_matches("minecraft:").to_string());
    }
    std::fs::write("/tmp/ours_state_join.txt", &details).unwrap();
    let _ = child.kill();
    let count = |n: &str| names.iter().filter(|x| x.as_str() == n).count();
    assert_eq!(count("set_player_team"), 2, "{names:?}");
    assert!(count("set_objective") >= 3 && count("set_display_objective") >= 3 && count("set_score") >= 3, "{names:?}");
    assert_eq!(count("boss_event"), 1, "{names:?}");
    assert_eq!(count("initialize_border"), 1);
    let pos = |n: &str| names.iter().position(|x| x == n).unwrap();
    assert!(pos("initialize_border") > pos("set_player_team"), "scoreboard packets precede the border");
}
