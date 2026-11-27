//! `/tick freeze`, summons while frozen and `/tick sprint` from the console, observed on the vanilla server and on OUR
//! server: how many game ticks the sprint runs (game time) and what entities that are ticked by it look like (the air of
//! water animals out of water, the motion of a squid and of an armor stand).
//!
//! A sprint that starts from a frozen server runs one normal tick first (the loop checks `runGameElements` of the tick
//! before the command), so 100 ticks sprint to 101 game ticks.
//!
//! `tick_sprint_dual_vanilla` runs the script against the real server and checks it against
//! `tests/fixtures/tick_sprint_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them); `tick_sprint_dual_ours` runs it on `mc-server`.
use std::time::{Duration, Instant};

use crate::common::join;
use crate::kick_dual::{Case, case, compare_ours, record_vanilla};
use mc_testkit::Server;

const DIR: &str = "tick_sprint_dual";
const ONLY: &str = "TICK_SPRINT_DUAL_ONLY";

const PROPS: &[&str] = &["allow-flight=true"];

const CASES: &[Case] = &[case!(frozen_summon_sprint, PROPS)];

const SUMMON: &str = "{NoAI:1b,Silent:1b,Invulnerable:1b,PersistenceRequired:1b}";

/// The text of the log line that `command` makes the server print (after the `]: ` of the log prefix); waits for the line
/// that contains `needle`.
fn ask(server: &Server, command: &str, needle: &str) -> String {
    let before = server.log().lines().count();
    server.cmd(command);
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let log = server.log();
        if let Some(line) = log.lines().skip(before).find(|l| l.contains(needle)) {
            return line
                .split_once("]: ")
                .map_or(line, |(_, text)| text)
                .to_string();
        }
        assert!(Instant::now() < deadline, "no `{needle}` after `{command}`");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn data(server: &Server, kind: &str, path: &str) -> String {
    ask(
        server,
        &format!("data get entity @e[type={kind},limit=1] {path}"),
        "entity data",
    )
}

fn summon_all(server: &Server) {
    for (i, kind) in ["cod", "squid", "armor_stand", "item"].iter().enumerate() {
        let extra = if *kind == "item" {
            "{Item:{id:\"minecraft:stone\",count:1}}".to_string()
        } else {
            SUMMON.to_string()
        };
        ask(
            server,
            &format!("summon minecraft:{kind} {}.5 -60 0.5 {extra}", i * 4),
            "Summoned",
        );
    }
}

/// The game time (the bot's login makes the absolute value run-dependent, so lines show the ticks since `base`).
fn game_time(server: &Server) -> i64 {
    let line = ask(server, "time query gametime", "The game time is");
    line.split("The game time is ")
        .nth(1)
        .and_then(|t| t.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| panic!("no game time in {line}"))
}

fn observe(server: &Server, out: &mut Vec<String>, label: &str, base: i64) {
    out.push(format!("{label}: ticks run {}", game_time(server) - base));
    for (kind, path) in [
        ("cod", "Air"),
        ("squid", "Air"),
        ("squid", "Motion"),
        ("armor_stand", "Motion"),
        ("item", "Motion"),
        ("item", "Age"),
    ] {
        out.push(format!(
            "{label}: {kind} {path}: {}",
            data(server, kind, path)
        ));
    }
}

async fn frozen_summon_sprint(server: &Server) -> Vec<String> {
    let mut out = Vec::new();
    let bot = join("127.0.0.1", server.port, "Steve").await;
    ask(server, "tick freeze", "frozen");
    let base = game_time(server);
    summon_all(server);
    observe(server, &mut out, "summoned", base);
    ask(server, "tick sprint 100", "Sprint completed");
    observe(server, &mut out, "after sprint", base);
    ask(server, "tick step 3", "Stepping");
    std::thread::sleep(Duration::from_millis(500));
    observe(server, &mut out, "after step", base);
    bot.disconnect().await;
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn tick_sprint_dual_vanilla() {
    record_vanilla(DIR, ONLY, CASES).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn tick_sprint_dual_ours() {
    compare_ours(DIR, ONLY, CASES).await;
}
