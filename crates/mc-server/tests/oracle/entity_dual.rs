//! Survival behaviour of vanilla vs OUR server through the same bot script (see `common/dual.rs`).
//! `entity_dual_vanilla`  runs the scenarios against the real vanilla server and checks them against the committed observations
//!                        in `tests/fixtures/entity_dual/*.txt` (`MC_UPDATE_FIXTURES=1` rewrites them; `ENTITY_DUAL_ONLY=name`
//!                        runs one scenario).
//! `entity_dual_ours`     runs the scenarios against `mc-server`, writes
//!                        `run/entity-dual-ours.diff` and fails on differences only with `ENTITY_DUAL_STRICT=1`.
use std::sync::Arc;

use crate::common::dual::*;
use crate::common::*;
use mc_testbot::harness::repo_root;

fn fixture_path(scenario: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/entity_dual")
        .join(format!("{scenario}.txt"))
}

fn only() -> Option<String> {
    std::env::var("ENTITY_DUAL_ONLY").ok()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server"]
async fn entity_dual_vanilla() {
    let server = Arc::new(
        mc_testbot::harness::vanilla(12345, "survival", PROPS)
            .start()
            .expect("start vanilla"),
    );
    let bot = join("127.0.0.1", server.port, "Bot").await;
    setup(&bot, &*server).await;
    let results = run_all(&bot, server.clone(), only().as_deref()).await;
    bot.disconnect().await;
    let update = std::env::var("MC_UPDATE_FIXTURES").is_ok_and(|v| v == "1");
    let mut failures = Vec::new();
    for (name, lines) in &results {
        let path = fixture_path(name);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, lines.join("\n") + "\n").unwrap();
            continue;
        }
        let expected: Vec<String> = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        let diffs = compare(&expected, lines);
        if !diffs.is_empty() {
            failures.push(format!("{name}:\n{}", diffs.join("\n")));
        }
    }
    assert!(
        failures.is_empty(),
        "vanilla observations differ from the committed fixtures:\n{}",
        failures.join("\n")
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the mc-server binary"]
async fn entity_dual_ours() {
    let server = Arc::new(
        ours(12345, "survival", PROPS)
            .start()
            .expect("start our server"),
    );
    let bot = join("127.0.0.1", server.port, "Bot").await;
    setup(&bot, &*server).await;
    let results = run_all(&bot, server.clone(), only().as_deref()).await;
    bot.disconnect().await;
    let mut report = String::new();
    for (name, lines) in &results {
        let expected: Vec<String> = std::fs::read_to_string(fixture_path(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        let diffs = compare(&expected, lines);
        if diffs.is_empty() {
            report.push_str(&format!("MATCH {name}\n"));
        } else {
            report.push_str(&format!("DIFF  {name}\n{}\n", diffs.join("\n")));
        }
    }
    let _ = std::fs::create_dir_all(repo_root().join("run"));
    let _ = std::fs::write(repo_root().join("run/entity-dual-ours.diff"), &report);
    println!("{report}");
    if std::env::var("ENTITY_DUAL_STRICT").is_ok() {
        assert!(
            !report.contains("DIFF "),
            "our server differs from vanilla:\n{report}"
        );
    }
}
