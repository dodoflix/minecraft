//! Joins OUR server (the `mc-server` binary) with the testbot and records the play packets up to the first chunk;
//! compared with the real server's sequence by the game crate's tests.
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_testbot::join::*;
use mc_testbot::log::PacketLog;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn join_our_server() {
    let bin = env!("CARGO_BIN_EXE_mc-server");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let dir = std::env::temp_dir().join(format!("mc-join-ours-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("eula.txt"), "eula=true\n").unwrap();
    std::fs::write(
        dir.join("server.properties"),
        format!("online-mode=false\nwhite-list=false\nserver-port={port}\nserver-ip=127.0.0.1\ngenerate-structures=false\nspawn-protection=0\nmax-tick-time=-1\nview-distance=2\nsimulation-distance=2\nenable-rcon=false\nenable-query=false\nrate-limit=0\nlevel-seed=12345\n"),
    )
    .unwrap();
    let mut child = Command::new(bin)
        .arg("nogui")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut all = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
            let _ = std::fs::write("/tmp/ours_server.log", &all);
            if line.contains("Done (") {
                let _ = tx.send(());
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(240))
        .expect("server start");
    tokio::time::sleep(Duration::from_secs(1)).await;

    let opts = BotOptions::new("127.0.0.1", port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log.clone()).await.unwrap();
    let result = login_and_configure(&mut conn, &opts).await;
    let mut out = format!(
        "configure: {:?}\n",
        result.as_ref().map(|_| ()).map_err(|e| e.to_string())
    );
    if result.is_ok() {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            let Ok(Ok(payload)) =
                tokio::time::timeout(Duration::from_millis(500), conn.reader.read_payload()).await
            else {
                continue;
            };
            let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap();
            let name = mc_protocol::packet::packet_name(
                mc_protocol::packet::ConnectionState::Play,
                mc_protocol::packet::PacketFlow::Clientbound,
                id,
            )
            .unwrap_or("?");
            out.push_str(&format!(
                "{id} {name} {} {}\n",
                payload.len(),
                payload
                    .iter()
                    .take(80)
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ));
            std::fs::write("/tmp/ours_join_seq.txt", &out).unwrap();
        }
    }
    std::fs::write("/tmp/ours_join_seq.txt", &out).unwrap();
    let _ = child.kill();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn join_vanilla_for_comparison() {
    let server = mc_testbot::harness::vanilla(12345, "survival", &[])
        .start()
        .expect("start vanilla");
    let opts = BotOptions::new("127.0.0.1", server.port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log.clone()).await.unwrap();
    login_and_configure(&mut conn, &opts).await.expect("cfg");
    let mut out = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let Ok(Ok(payload)) =
            tokio::time::timeout(Duration::from_millis(500), conn.reader.read_payload()).await
        else {
            continue;
        };
        let id = mc_protocol::codec::read_varint(&mut &payload[..]).unwrap();
        let name = mc_protocol::packet::packet_name(
            mc_protocol::packet::ConnectionState::Play,
            mc_protocol::packet::PacketFlow::Clientbound,
            id,
        )
        .unwrap_or("?");
        out.push_str(&format!(
            "{id} {name} {} {}\n",
            payload.len(),
            payload
                .iter()
                .take(80)
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
    }
    std::fs::write("/tmp/vanilla_join_hex.txt", &out).unwrap();
    server.stop();
}
