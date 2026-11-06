//! `/worldborder`, `/tick`, `/setworldspawn`, `/spawnpoint` through the same bot script against vanilla and OUR server:
//! the border/ticking/spawn/chat packets each command produces (packet debug forms, timestamps stripped, digits of
//! timing dependent tick/sprint reports masked) are compared.
//! `world_commands_vanilla`  runs the script on the real server and checks it against `tests/fixtures/world_commands_dual.txt`
//!                           (`WC_DUAL_UPDATE=1` rewrites the fixture).
//! `world_commands_ours`     runs it on `mc-server` and fails on any difference from the fixture.
use std::time::Duration;

use crate::common::dual::PROPS;
use crate::common::{join, ours};
use mc_testbot::Bot;
use mc_testbot::harness::repo_root;

const WATCHED: &[&str] = &[
    "play/initialize_border",
    "play/set_border_",
    "play/ticking_state",
    "play/ticking_step",
    "play/system_chat",
    "play/set_default_spawn_position",
];

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world_commands_dual.txt")
}

/// The watched clientbound packet lines logged since line `from` (timestamp prefix removed) and the new log length.
fn watched_since(bot: &Bot, from: usize) -> (usize, Vec<String>) {
    let text = bot.log_text();
    let lines: Vec<&str> = text.lines().collect();
    let out = lines[from.min(lines.len())..]
        .iter()
        .filter_map(|l| l.split_once("ms ").map(|(_, rest)| rest))
        .filter(|rest| rest.starts_with("<- ") && WATCHED.iter().any(|w| rest[3..].starts_with(w)))
        .map(str::to_string)
        .collect();
    (lines.len(), out)
}

/// Timing dependent reports: every digit becomes `#`.
fn mask(line: &str) -> String {
    if line.contains("commands.tick.query") || line.contains("commands.tick.sprint.report") {
        line.chars()
            .map(|c| if c.is_ascii_digit() { '#' } else { c })
            .collect()
    } else {
        line.to_string()
    }
}

const SCRIPT: &[(&str, u64)] = &[
    ("worldborder get", 4),
    ("worldborder set 10 5", 4),
    ("worldborder set 10 5", 4),
    ("worldborder set 10", 4),
    ("worldborder get", 4),
    ("worldborder set 0", 4),
    ("worldborder set 100000000", 4),
    ("worldborder set 200", 4),
    ("worldborder add 50", 4),
    ("worldborder add -10 3", 4),
    ("worldborder add 1000 10", 4),
    ("worldborder center 100 -50", 4),
    ("worldborder center 100 -50", 4),
    ("worldborder center 30000000 0", 4),
    ("worldborder center ~ ~", 4),
    ("worldborder damage amount 0.5", 4),
    ("worldborder damage amount 0.5", 4),
    ("worldborder damage buffer 3", 4),
    ("worldborder damage buffer 3.25", 4),
    ("worldborder warning distance 7", 4),
    ("worldborder warning distance 7", 4),
    ("worldborder warning time 20", 4),
    ("worldborder warning time 5s", 4),
    ("worldborder warning time 5s", 4),
    ("worldborder set 59999970", 4),
    ("worldborder get", 4),
    ("tick query", 4),
    ("tick step", 4),
    ("tick freeze", 4),
    ("tick freeze", 4),
    ("tick query", 4),
    ("tick step 5", 12),
    ("tick step 40", 4),
    ("tick step stop", 4),
    ("tick step stop", 4),
    ("tick unfreeze", 4),
    ("tick rate 10", 4),
    ("tick rate 10.5", 4),
    ("tick query", 4),
    ("tick rate 20", 4),
    ("tick freeze", 4),
    ("tick sprint 20", 4),
    ("tick sprint stop", 4),
    ("tick unfreeze", 4),
    ("tick sprint 20", 4),
    ("tick sprint 60", 4),
    ("tick sprint stop", 4),
    ("tick sprint stop", 4),
    ("setworldspawn 10 70 10", 4),
    ("setworldspawn 10 70 10", 4),
    ("setworldspawn 3 4 5 45 30", 4),
    ("setworldspawn ~ ~ ~", 4),
    ("spawnpoint Bot 1 2 3", 4),
    ("spawnpoint Bot 1 2 3 90 -10", 4),
    ("spawnpoint @a", 4),
];

async fn run(bot: &Bot, console: &dyn Fn(&str)) -> Vec<String> {
    console("op Bot");
    bot.wait_ticks(10).await.expect("op");
    let mut out = Vec::new();
    for (command, ticks) in SCRIPT {
        let (from, _) = watched_since(bot, usize::MAX);
        bot.command(command);
        bot.wait_ticks(*ticks).await.expect("command");
        if command.starts_with("tick sprint ") && !command.ends_with("stop") {
            for _ in 0..200 {
                if bot
                    .log_text()
                    .lines()
                    .skip(from)
                    .any(|l| l.contains("commands.tick.sprint.report"))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            bot.wait_ticks(4).await.expect("after sprint");
        }
        let (_, lines) = watched_since(bot, from);
        out.push(format!("> {command}"));
        out.extend(lines.iter().map(|l| mask(l)));
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn world_commands_vanilla() {
    let server = mc_testbot::harness::vanilla(12345, "survival", PROPS)
        .start()
        .expect("start vanilla");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    let lines = run(&bot, &|c| server.cmd(c)).await;
    bot.disconnect().await;
    server.stop();
    if std::env::var("WC_DUAL_UPDATE").is_ok() {
        std::fs::write(fixture(), lines.join("\n") + "\n").unwrap();
        return;
    }
    let expected = std::fs::read_to_string(fixture()).unwrap_or_default();
    assert_eq!(
        expected.trim_end(),
        lines.join("\n"),
        "vanilla output differs from the committed fixture"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn world_commands_ours() {
    let server = ours(12345, "survival", PROPS)
        .start()
        .expect("start our server");
    let bot = join("127.0.0.1", server.port, "Bot").await;
    let lines = run(&bot, &|c| server.cmd(c)).await;
    bot.disconnect().await;
    let expected: Vec<String> = std::fs::read_to_string(fixture())
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let _ = std::fs::create_dir_all(repo_root().join("run"));
    let _ = std::fs::write(
        repo_root().join("run/world-commands-ours.txt"),
        lines.join("\n") + "\n",
    );
    let mut diffs = Vec::new();
    for i in 0..expected.len().max(lines.len()) {
        let (e, o) = (
            expected.get(i).map(String::as_str).unwrap_or("<none>"),
            lines.get(i).map(String::as_str).unwrap_or("<none>"),
        );
        if e != o {
            diffs.push(format!("#{i}\n  vanilla: {e}\n  ours:    {o}"));
        }
    }
    assert!(
        diffs.is_empty(),
        "our server differs from vanilla:\n{}",
        diffs.join("\n")
    );
}
