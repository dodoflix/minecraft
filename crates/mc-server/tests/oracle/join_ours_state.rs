//! Joins OUR server on a world with scoreboard / custom boss event / world border saved data (saved by the vanilla
//! server from console commands and copied over) and checks the matching join packets arrive.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::common::join;
use mc_testbot::harness::vanilla;
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

/// Saves a world with two teams, three objectives on the list/sidebar/below-name slots with a score each, a custom boss
/// bar shown to `Bot` and a world border in `dir` (the vanilla server runs there).
async fn vanilla_seed_world(dir: &std::path::Path) {
    let server = vanilla(12345, "survival", &[])
        .dir(dir)
        .start()
        .expect("start vanilla");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    for command in [
        "team add red",
        "team add blue",
        "scoreboard objectives add objA dummy",
        "scoreboard objectives add objB dummy",
        "scoreboard objectives add objC dummy",
        "scoreboard objectives setdisplay list objA",
        "scoreboard objectives setdisplay sidebar objB",
        "scoreboard objectives setdisplay below_name objC",
        "scoreboard players set Bot objA 1",
        "scoreboard players set Bot objB 2",
        "scoreboard players set Bot objC 3",
        "bossbar add minecraft:seed Seed",
        "bossbar set minecraft:seed players Bot",
        "worldborder set 100",
        "save-all flush",
    ] {
        server.cmd(command);
    }
    server.wait_log("Saved the game", 60).expect("saved");
    bot.disconnect().await;
    assert!(server.stop(), "vanilla did not stop\n{}", server.log());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
async fn join_with_scoreboard_boss_bar_and_border() {
    let seed = tempfile::tempdir().expect("temp dir");
    vanilla_seed_world(seed.path()).await;
    let server = crate::common::ours(
        12345,
        "survival",
        &["view-distance=2", "simulation-distance=2"],
    )
    .prepare({
        let world = seed.path().join("world");
        move |dir| copy_dir(&world, &dir.join("world"))
    })
    .start()
    .expect("start ours");
    let opts = BotOptions::new("127.0.0.1", server.port, "Bot");
    let log = Arc::new(Mutex::new(PacketLog::new()));
    let mut conn = open(&opts, log.clone()).await.unwrap();
    login_and_configure(&mut conn, &opts)
        .await
        .expect("configuration");
    let mut names: Vec<String> = Vec::new();
    let mut details = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
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
        details.push_str(&format!(
            "{name} {}\n",
            payload
                .iter()
                .take(60)
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        names.push(name.trim_start_matches("minecraft:").to_string());
    }
    std::fs::write("/tmp/ours_state_join.txt", &details).unwrap();
    server.kill();
    let count = |n: &str| names.iter().filter(|x| x.as_str() == n).count();
    assert_eq!(count("set_player_team"), 2, "{names:?}");
    assert!(
        count("set_objective") >= 3
            && count("set_display_objective") >= 3
            && count("set_score") >= 3,
        "{names:?}"
    );
    assert_eq!(count("boss_event"), 1, "{names:?}");
    assert_eq!(count("initialize_border"), 1);
    let pos = |n: &str| names.iter().position(|x| x == n).unwrap();
    assert!(
        pos("initialize_border") > pos("set_player_team"),
        "scoreboard packets precede the border"
    );
}
