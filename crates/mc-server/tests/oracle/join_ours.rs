//! Joins OUR server (the `mc-server` binary) with the testbot and records the play packets up to the first chunk;
//! compared with the real server's sequence by the game crate's tests.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mc_testbot::join::*;
use mc_testbot::log::PacketLog;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn join_our_server() {
    let server = crate::common::ours(
        12345,
        "survival",
        &["view-distance=2", "simulation-distance=2"],
    )
    .start()
    .expect("start ours");
    let opts = BotOptions::new("127.0.0.1", server.port, "Bot");
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
    server.kill();
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
