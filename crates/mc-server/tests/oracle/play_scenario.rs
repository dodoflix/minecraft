//! Differential play scenario: the same scripted client (join, idle, legit movement incl. jump/sprint, speed hack,
//! teleport confirm, chat, commands, keep-alive) against the real vanilla server and against OUR `mc-server`.
//! The packet sequence the client receives, reduced to what the script causes (`normalise`), is committed in
//! `tests/fixtures/play_scenario/transcript.txt`. `play_scenario_vanilla` checks the vanilla recording against it
//! (`MC_UPDATE_FIXTURES=1` rewrites it) and `play_scenario_ours` checks our server against it. On a mismatch the
//! received transcript is written next to the target directory's oracle dumps (`common::dump_path`).
use std::time::Duration;

use mc_protocol::packets::play::*;
use mc_testbot::join::BotOptions;
use mc_testbot::raw::RawClient;

/// The flat world puts the spawn on a known floor, so the script does not depend on terrain under a random spawn.
const PROPS: &[&str] = &["level-type=minecraft:flat", "generate-structures=false"];

/// Packets that arrive at times the script does not control: entity updates and mobs, random block ticks, the world
/// clock, chunk streaming.
const UNSCRIPTED: &[&str] = &[
    "minecraft:add_entity",
    "minecraft:block_update",
    "minecraft:bundle_delimiter",
    "minecraft:chunk_batch_finished",
    "minecraft:chunk_batch_start",
    "minecraft:entity_event",
    "minecraft:entity_position_sync",
    "minecraft:forget_level_chunk",
    "minecraft:keep_alive",
    "minecraft:level_event",
    "minecraft:level_chunk_with_light",
    "minecraft:light_update",
    "minecraft:move_entity_pos",
    "minecraft:move_entity_pos_rot",
    "minecraft:move_entity_rot",
    "minecraft:remove_entities",
    "minecraft:rotate_head",
    "minecraft:section_blocks_update",
    "minecraft:set_chunk_cache_center",
    "minecraft:set_entity_data",
    "minecraft:set_entity_motion",
    "minecraft:set_time",
    "minecraft:sound",
    "minecraft:teleport_entity",
    "minecraft:update_attributes",
];

/// The latency refresh runs on the server's own 600 tick clock, not the script's: only the join and the game mode
/// change are expected to send player info.
const PLAYER_INFO_SECTIONS: [&str; 2] = ["## join", "## cmd gamemode creative"];

/// Translation keys of a chat packet line: the text itself carries the time of day and the like.
fn keys(line: &str) -> String {
    line.split("key: \"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect::<Vec<_>>()
        .join(",")
}

/// Section markers and the packets the script causes, in order: names only (the spawn point of a new player is random,
/// and so are the ids and positions of the corrections), chat packets with their translation keys, a disconnect with
/// its text. Consecutive repeats collapse to `name *`.
fn normalise(transcript: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut section = "";
    for line in transcript.lines() {
        let name = line.split(' ').next().unwrap_or("");
        let kept = if line.starts_with("## ") {
            section = line;
            line.to_owned()
        } else if UNSCRIPTED.contains(&name)
            || (name == "minecraft:player_info_update" && !PLAYER_INFO_SECTIONS.contains(&section))
        {
            continue;
        } else if name == "minecraft:system_chat" {
            format!("{name} {}", keys(line))
        } else if name == "minecraft:disconnect" {
            line.to_owned()
        } else {
            name.to_owned()
        };
        match out.last_mut() {
            Some(last) if *last == kept && !kept.starts_with("## ") => *last = format!("{kept} *"),
            Some(last) if *last == format!("{kept} *") => {}
            _ => out.push(kept),
        }
    }
    out.join("\n") + "\n"
}

async fn scenario(port: u16, op: impl Fn(&str)) -> String {
    let mut opts = BotOptions::new("127.0.0.1", port, "E2EBot");
    opts.timeout = Duration::from_secs(120);
    let mut c = RawClient::connect(&opts).await.expect("join");
    // join: wait for the first position (spawn), which the client confirms, and for the first tick's state sync
    // (the experience packet closes it), then load
    c.section("join");
    c.pump_until("minecraft:set_experience", 1, Duration::from_secs(120))
        .await
        .expect("join");
    c.send(ServerboundPlayPacket::PlayerLoaded(
        ServerboundPlayerLoadedPacket,
    ))
    .await
    .expect("player loaded");
    c.section("idle 8 s");
    let (x0, y0, z0) = c.pos;
    c.pump(Duration::from_secs(8), |n| {
        (n % 20 == 0).then_some((x0, y0, z0, true))
    })
    .await
    .expect("idle");
    c.section("walk 20 ticks");
    c.pump(Duration::from_millis(1000), |n| {
        Some((x0, y0, z0 + 0.2158 * f64::from(n + 1), true))
    })
    .await
    .expect("walk");
    let (x1, y1, z1) = c.pos;
    c.section("jump");
    let mut vy = 0.42f64;
    let mut y = y1;
    c.pump(Duration::from_millis(1500), |n| {
        if n >= 12 + 8 && y <= y1 {
            return None;
        }
        y += vy;
        vy = (vy - 0.08) * 0.98;
        let g = y <= y1;
        if g {
            y = y1;
        }
        Some((x1, y, z1 + 0.2806 * f64::from(n + 1), g))
    })
    .await
    .expect("jump");
    let (x2, y2, z2) = c.pos;
    c.section("speed hack");
    c.pump(Duration::from_millis(100), |n| {
        (n == 0).then_some((x2 + 500.0, y2, z2, true))
    })
    .await
    .expect("speed hack");
    c.pump(Duration::from_millis(1500), |_| None)
        .await
        .expect("after the speed hack");
    c.section("chat+commands");
    c.send(ServerboundPlayPacket::Chat(ServerboundChatPacket {
        message: "hello".into(),
        time_stamp: mc_protocol::codec::Instant(0),
        salt: 0,
        signature: None,
        last_seen_messages: LastSeenMessagesUpdate {
            offset: 0,
            acknowledged: Default::default(),
            checksum: 0,
        },
    }))
    .await
    .expect("chat");
    c.pump(Duration::from_millis(500), |_| None)
        .await
        .expect("chat reply");
    op("op E2EBot");
    c.pump(Duration::from_millis(1000), |_| None)
        .await
        .expect("op");
    for command in [
        "time query gametime",
        "gamemode creative",
        "tp @s ~ ~5 ~",
        "say hi",
    ] {
        c.send(ServerboundPlayPacket::ChatCommand(
            ServerboundChatCommandPacket {
                command: command.into(),
            },
        ))
        .await
        .expect("command");
        c.section(&format!("cmd {command}"));
        c.pump(Duration::from_millis(1000), |_| None)
            .await
            .expect(command);
    }
    // the server's own 15 s keep-alive clock decides where a keep-alive falls: wait for the second one (the client
    // answers each) instead of recording where they land
    c.section("keep-alives");
    c.pump_until("minecraft:keep_alive", 2, Duration::from_secs(60))
        .await
        .expect("keep-alive");
    normalise(&c.transcript)
}

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/play_scenario/transcript.txt")
}

/// The first differing line, with the dump of the whole received transcript.
fn assert_matches_fixture(who: &str, received: &str) {
    let expected =
        std::fs::read_to_string(fixture()).expect("tests/fixtures/play_scenario/transcript.txt");
    if expected == received {
        return;
    }
    let dump = crate::common::dump_path(&format!("play_scenario_{who}.txt"));
    std::fs::write(&dump, received).expect("write the transcript dump");
    let (mut want, mut got) = (expected.lines(), received.lines());
    let mut line = 1;
    let diff = loop {
        match (want.next(), got.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (a, b) => break format!("line {line}: expected {a:?}, received {b:?}"),
        }
    };
    panic!(
        "{who} differs from the committed transcript at {diff}; received transcript: {}",
        dump.display()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn play_scenario_vanilla() {
    let server = mc_testbot::harness::vanilla(12345, "survival", PROPS)
        .start()
        .expect("start vanilla");
    let received = scenario(server.port, |c| server.cmd(c)).await;
    server.stop();
    if std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(fixture().parent().unwrap()).unwrap();
        std::fs::write(fixture(), received).unwrap();
    } else {
        assert_matches_fixture("vanilla", &received);
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn play_scenario_ours() {
    let server = crate::common::ours(12345, "survival", PROPS)
        .start()
        .expect("start ours");
    let received = scenario(server.port, |c| server.cmd(c)).await;
    server.kill();
    assert_matches_fixture("ours", &received);
}
