//! `/jfr start` ... `/jfr stop` on the real vanilla server and on `mc-server`: the recording is read by the JDK's own
//! `jdk.jfr.consumer.RecordingFile` and, through vanilla's `JfrStatsParser`, turned into the summary JSON
//! (`JfrOracle.java`), so a file the JDK cannot read fails here and so does a summary that is not what vanilla's own
//! serializer writes for the same recording. Compared between the two servers: the `minecraft.*` event types (names,
//! labels, descriptions, categories, settings, fields) and which of them have events, the structure of the summary
//! JSON (key order, value kinds, bare `NaN`), the file names, the log lines.
//!
//! Accepted differences (decided: the closest analogue where one exists, the rest stays different): the JVM's own
//! events (`jdk.*`) have no Rust source beyond the stand-ins the profiler writes (CPU load, heap summaries; no
//! per-thread allocation, collections or file IO events), so `threadAllocation` (the threads of a JVM) and the file
//! IO contributors (`fileIO.read` / `fileIO.write`, the JVM's own file reads and writes) are left out of the shape
//! comparison; events name no thread and no stack trace (their references are `null`); and the values in a summary
//! (durations, sizes) are the run's own.
use mc_testbot::harness::{self, repo_root};
use mc_testkit::Server;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// What one server produced for a recording.
struct Recording {
    /// `JfrOracle types` for the `.jfr` file.
    types: String,
    /// The `jfr-report-*.json` the server wrote.
    summary: String,
    /// `JfrOracle summary` (vanilla's parser and serializer) on the `.jfr` file.
    summary_by_vanilla: String,
    /// `JfrOracle chunks`: the chunk region events in the order the parser reads them.
    chunk_events: String,
    /// File names, digits masked.
    files: Vec<String>,
    /// The log lines about the recording, timestamps and paths masked.
    log: Vec<String>,
}

fn mask_digits(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}

/// Gson writes a `NaN` or an infinite double as a bare token, which is not JSON: the tokens outside strings become
/// the strings `"NaN"` / `"Infinity"` / `"-Infinity"` marked with a control character, so `shape` can tell them from
/// numbers.
fn lenient(text: &str) -> serde_json::Value {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if in_string {
            out.push(c);
            let was_escaped = escaped;
            escaped = !was_escaped && c == '\\';
            if c == '"' && !was_escaped {
                in_string = false;
            }
            rest = &rest[c.len_utf8()..];
            continue;
        }
        if c == '"' {
            in_string = true;
        }
        let token = ["NaN", "-Infinity", "Infinity"]
            .into_iter()
            .find(|t| rest.starts_with(t));
        match token {
            Some(token) => {
                out.push_str(&format!("\"\u{1}{token}\""));
                rest = &rest[token.len()..];
            }
            None => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("summary is not JSON: {e}\n{text}"))
}

/// `{"a":1,"b":["x"]}` -> `{a:number,b:[string]}` in the order the members were written.
fn shape(value: &serde_json::Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(_) => "bool".into(),
        Value::Number(_) => "number".into(),
        Value::String(s) => s.strip_prefix('\u{1}').unwrap_or("string").to_owned(),
        Value::Array(items) => {
            let mut shapes: Vec<String> = items.iter().map(shape).collect();
            shapes.dedup();
            format!("[{}]", shapes.join("|"))
        }
        Value::Object(map) => {
            let members: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}:{}", shape(v)))
                .collect();
            format!("{{{}}}", members.join(","))
        }
    }
}

/// `JfrOracle.java <mode> <file>` on vanilla's classes.
fn oracle(mode: &str, jfr: &Path) -> String {
    let root = repo_root();
    let out = Command::new(root.join("scripts/vanilla-java.sh"))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/JfrOracle.java"))
        .arg(mode)
        .arg(jfr)
        .output()
        .expect("run scripts/vanilla-java.sh (needs ref/ and java)");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "JfrOracle {mode} could not read {}:\n{}\n{}",
        jfr.display(),
        text,
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

fn debug_files(server: &Server) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(server.dir().join("debug"))
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.sort();
    files
}

async fn record(server: &Server) -> Recording {
    server.cmd("jfr start");
    server
        .wait_log("JFR profiling started", 60)
        .expect("jfr started");
    let bot = crate::common::join("127.0.0.1", server.port, "Bot").await;
    bot.chat("hello recorder");
    // fresh chunks: generation, region writes by the save below
    server.cmd("tp Bot 3000 120 3000");
    tokio::time::sleep(Duration::from_secs(15)).await;
    bot.disconnect().await;
    server.cmd("save-all flush");
    server.wait_log("Saved the game", 120).expect("saved");
    server.cmd("jfr stop");
    server
        .wait_log("JFR profiling stopped", 120)
        .expect("jfr stopped");
    // the summary is written when the recording was dumped
    server
        .wait_log("Dumped recording summary to", 120)
        .expect("summary written");
    let files = debug_files(server);
    let jfr = files
        .iter()
        .find(|f| f.extension().is_some_and(|e| e == "jfr"))
        .expect("a .jfr file");
    let report = files
        .iter()
        .find(|f| f.to_string_lossy().contains("jfr-report-"))
        .expect("a summary file");
    let log = server
        .log()
        .lines()
        .filter(|l| {
            l.contains("flight recorder")
                || l.contains("recording summary")
                || l.contains("JFR profiling")
        })
        .map(|l| {
            let text = l.split_once("]: ").map_or(l, |(_, rest)| rest);
            let text = text.split(" to ./debug").next().unwrap_or(text);
            mask_digits(text)
        })
        .collect();
    Recording {
        types: oracle("types", jfr),
        summary: std::fs::read_to_string(report).expect("summary"),
        summary_by_vanilla: oracle("summary", jfr),
        chunk_events: oracle("chunks", jfr),
        files: files
            .iter()
            .map(|f| mask_digits(&f.file_name().unwrap().to_string_lossy()))
            .collect(),
        log,
    }
}

/// Fails with the lines that differ, which `assert_eq!` cannot show for a long text.
fn lines_equal(left: &str, right: &str, what: &str) -> Result<(), String> {
    let differing: Vec<String> = left
        .lines()
        .zip(right.lines())
        .enumerate()
        .filter(|(_, (l, r))| l != r)
        .map(|(i, (l, r))| format!("line {}: {l:?} vs {r:?}", i + 1))
        .collect();
    if differing.is_empty() && left.lines().count() == right.lines().count() {
        return Ok(());
    }
    Err(format!(
        "{what}: {} lines vs {}\n{}",
        left.lines().count(),
        right.lines().count(),
        differing.join("\n")
    ))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "oracle: starts the vanilla server, the mc-server binary and java"]
async fn recording_equals_vanilla() {
    let vanilla_server = harness::vanilla(7, "survival", &[])
        .start()
        .expect("vanilla");
    let vanilla = record(&vanilla_server).await;
    vanilla_server.stop();
    drop(vanilla_server);
    // the harness turns structures off on our side by default; vanilla generates them, so the recording of both has them
    let ours_server = crate::common::ours(7, "survival", &["generate-structures=true"])
        .start()
        .expect("ours");
    let ours = record(&ours_server).await;
    ours_server.stop();
    println!("{}", vanilla.types);
    println!("{}", shape(&lenient(&vanilla.summary)));
    println!(
        "ours, chunk events in reading order:\n{}",
        ours.chunk_events
    );
    // the harness: vanilla's own summary is what its serializer writes for its own file
    let mut problems = Vec::new();
    if vanilla.summary != vanilla.summary_by_vanilla {
        problems
            .push("vanilla's summary differs from its serializer on the same recording".to_owned());
    }
    if let Err(e) = lines_equal(
        &ours.summary,
        &ours.summary_by_vanilla,
        "our summary (left) vs vanilla's serializer on our recording (right)",
    ) {
        problems.push(e);
    }
    if vanilla.files != ours.files {
        problems.push(format!(
            "files (vanilla left, ours right): {:?} vs {:?}",
            vanilla.files, ours.files
        ));
    }
    if vanilla.log != ours.log {
        problems.push(format!("log lines: {:?} vs {:?}", vanilla.log, ours.log));
    }
    if let Err(e) = lines_equal(
        &vanilla.types,
        &ours.types,
        "event types (vanilla left, ours right)",
    ) {
        problems.push(e);
    }
    let shape_of = |summary: &str| {
        let mut value = lenient(summary);
        if let Some(file_io) = value.get_mut("fileIO").and_then(Value::as_object_mut) {
            for key in ["read", "write"] {
                if let Some(section) = file_io.get_mut(key).and_then(Value::as_object_mut) {
                    section.shift_remove("topContributors");
                }
            }
        }
        value
            .as_object_mut()
            .map(|o| o.shift_remove("threadAllocation"));
        shape(&value)
    };
    if shape_of(&vanilla.summary) != shape_of(&ours.summary) {
        problems.push(format!(
            "summary shape (vanilla left, ours right):\n{}\n{}",
            shape_of(&vanilla.summary),
            shape_of(&ours.summary)
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n---\n"));
}
