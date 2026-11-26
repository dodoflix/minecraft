//! Statistics, advancement and legacy structure index files in every shape a hand edit or a crash leaves. The real 26.3
//! server and `mc-server` each start on an empty world, one bot per stats or advancements file joins (so the file is
//! read), or on a copy of an old world whose `data/Mineshaft.dat` is damaged (read by the file fixer); the warnings
//! and errors of the two logs, with the first line of the exception behind each, are compared in order. Run with
//! `cargo test -p mc-server --test oracle damaged_ -- --ignored --nocapture`.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::entity_persistence::ask;
use crate::startup_gates::gate_log;
use crate::world_start::{Run, Side, cache_dir, copy_dir, first_log_difference, log_records, run};

const VERSION: &str = "5023";

/// A stats file with these entries of the type `minecraft:custom`.
fn stats(entries: &str) -> String {
    format!(r#"{{"stats":{{"minecraft:custom":{{{entries}}}}},"DataVersion":{VERSION}}}"#)
}

/// The number shapes `Codec.INT` reads out of a stats file, at the version that runs the fixer and at the current one.
fn number_edges(version: &str) -> String {
    format!(
        r#"{{"stats":{{"minecraft:custom":{{"minecraft:jump":1.5,"minecraft:walk_one_cm":3000000000,"minecraft:sprint_one_cm":1e2,"minecraft:fly_one_cm":-7,"minecraft:swim_one_cm":99999999999999999999,"minecraft:fall_one_cm":true,"minecraft:crouch_one_cm":null,"minecraft:boat_one_cm":"5","minecraft:aviate_one_cm":1.9999999999999999999,"minecraft:climb_one_cm":-1.5,"minecraft:walk_on_water_one_cm":12345678901234567890}}}},"DataVersion":{version}}}"#
    )
}

const PROGRESS: &str = r#"{"minecraft:story/root":{"criteria":{"crafting_table":"2020-01-01 00:00:00 +0000"},"done":true},"DataVersion":4903}"#;

/// `(folder, case, file content)`.
fn cases() -> Vec<(&'static str, &'static str, String)> {
    let valid = stats(r#""minecraft:jump":5,"minecraft:walk_one_cm":100"#);
    let cut = |text: &str| text[..text.len() / 2].to_owned();
    vec![
        ("stats", "bom", format!("\u{feff}{valid}")),
        ("stats", "empty_file", String::new()),
        ("stats", "blank", "  \n".to_owned()),
        ("stats", "truncated", cut(&valid)),
        ("stats", "nan", stats(r#""minecraft:jump":NaN"#)),
        ("stats", "wrong_types", r#"{"stats":5,"DataVersion":"x"}"#.to_owned()),
        (
            "stats",
            "inner_wrong",
            r#"{"stats":{"minecraft:custom":"x","minecraft:used":{"minecraft:zzz":1,"minecraft:stone":"q"}},"DataVersion":1343}"#.to_owned(),
        ),
        ("stats", "array_root", "[1,2]".to_owned()),
        (
            "stats",
            "bad_keys",
            format!(
                r#"{{"stats":{{"Minecraft:Custom":{{"minecraft:jump":1}},"minecraft:zzz":{{}},"minecraft:custom":{{"Minecraft:Jump":1,"minecraft:zzz":2,"minecraft:walk_one_cm":3}}}},"DataVersion":{VERSION}}}"#
            ),
        ),
        (
            "stats",
            "duplicate_keys_current",
            format!(
                r#"{{"stats":{{"minecraft:custom":{{"minecraft:jump":1,"minecraft:jump":2}}}},"DataVersion":{VERSION}}}"#
            ),
        ),
        (
            "stats",
            "old_flat_format",
            r#"{"stat.leaveGame":3,"stat.playOneMinute":100,"achievement.openInventory":1,"stat.mineBlock.minecraft.stone":5}"#.to_owned(),
        ),
        ("stats", "trailing_comma", r#"{"stats":{},"DataVersion":1343,}"#.to_owned()),
        ("stats", "comment", "/* c */ {\"stats\":{}}".to_owned()),
        ("stats", "trailing_data", format!("{valid} x")),
        // the bytes are written by the test: UTF-8 is malformed
        ("stats", "invalid_utf8", String::new()),
        ("advancements", "invalid_utf8", String::new()),
        ("stats", "late_invalid_utf8", String::new()),
        ("stats", "syntax_error_before_late_invalid_utf8", String::new()),
        ("advancements", "late_invalid_utf8", String::new()),
        ("advancements", "syntax_error_before_late_invalid_utf8", String::new()),
        ("stats", "invalid_utf8_after_document", String::new()),
        (
            "stats",
            "duplicate_keys",
            r#"{"stats":{"minecraft:custom":{"minecraft:jump":1,"minecraft:jump":2}},"stats":{"minecraft:custom":{"minecraft:jump":3}},"DataVersion":1343}"#.to_owned(),
        ),
        ("stats", "number_edges_current", number_edges(VERSION)),
        ("stats", "number_edges_1343", number_edges("1343")),
        // a JSON null entry makes the data fixer throw while it reads a stats file of 1452..5011 or the criteria of a typed
        // advancement of up to 4768: the exception leaves the player's constructor, the player is kicked and not placed
        (
            "stats",
            "null_entry_4325",
            r#"{"stats":{"minecraft:custom":{"minecraft:jump":null,"minecraft:play_time":5}},"DataVersion":4325}"#.to_owned(),
        ),
        (
            "stats",
            "null_entry_5012",
            r#"{"stats":{"minecraft:custom":{"minecraft:jump":null,"minecraft:play_time":5}},"DataVersion":5012}"#.to_owned(),
        ),
        (
            "advancements",
            "null_criterion_4189",
            r#"{"minecraft:adventure/adventuring_time":{"criteria":{"minecraft:deep_dark":null},"done":false},"DataVersion":4189}"#.to_owned(),
        ),
        (
            "advancements",
            "null_criterion_4769",
            r#"{"minecraft:adventure/adventuring_time":{"criteria":{"minecraft:deep_dark":null},"done":false},"DataVersion":4769}"#.to_owned(),
        ),
        ("advancements", "bom", format!("\u{feff}{PROGRESS}")),
        ("advancements", "empty_file", String::new()),
        ("advancements", "truncated", cut(PROGRESS)),
        ("advancements", "trailing_comma", r#"{"DataVersion":1343,}"#.to_owned()),
        ("advancements", "wrong_types", r#"{"minecraft:story/root":5,"DataVersion":"x"}"#.to_owned()),
        ("advancements", "array_root", "[1]".to_owned()),
        (
            "advancements",
            "duplicate_keys",
            r#"{"minecraft:story/root":{"criteria":{"crafting_table":"2020-01-01 00:00:00 +0000"},"done":true},"minecraft:story/root":{"criteria":{},"done":false},"DataVersion":1343}"#.to_owned(),
        ),
        (
            "advancements",
            "old_names",
            r#"{"minecraft:husbandry/obtain_netherite_hoe":{"criteria":{"netherite_hoe":"2020-01-01 00:00:00 +0000"},"done":true},"minecraft:nether/summon_wither":{"criteria":{"summoned":"2020-01-01 00:00:00 +0000"},"done":true},"DataVersion":1343}"#.to_owned(),
        ),
        (
            "advancements",
            "unknown",
            r#"{"minecraft:story/zzz":{"criteria":{"a":"2020-01-01 00:00:00 +0000"},"done":true},"minecraft:story/root":{"criteria":{"zzz":"2020-01-01 00:00:00 +0000"},"done":false},"DataVersion":1343}"#.to_owned(),
        ),
        (
            "advancements",
            "bad_date",
            r#"{"minecraft:story/root":{"criteria":{"crafting_table":"yesterday"},"done":true},"DataVersion":1343}"#.to_owned(),
        ),
    ]
}

/// The files whose text is not UTF-8, which the cases cannot hold as strings. The decoder fails on a read of 8192 chars,
/// so a failure late in a file only shows when the parser needs the chars of that read.
fn invalid_utf8_bytes(name: &str) -> Option<Vec<u8>> {
    let padded = |text: &str, spaces: usize| {
        let mut bytes = text.as_bytes().to_vec();
        bytes.extend(std::iter::repeat_n(b' ', spaces));
        bytes.extend([0xff, 0xfe]);
        bytes
    };
    Some(match name {
        "invalid_utf8" => b"{\"a\":\"\xff\xfe\"}".to_vec(),
        // a whole document, and then more than a read of blanks before the bad bytes
        "late_invalid_utf8" => padded("{}", 9000),
        "syntax_error_before_late_invalid_utf8" => padded("{\"a\":,", 9000),
        // the bad bytes in the first read, after the document: nothing of that read is delivered
        "invalid_utf8_after_document" => padded("{}", 100),
        _ => return None,
    })
}

fn player_name(index: usize) -> String {
    format!("Bot{index}")
}

fn write_files(dir: &Path, cases: &[(&str, &str, Vec<u8>)]) {
    for (index, (folder, _, bytes)) in cases.iter().enumerate() {
        let uuid = mc_core::uuid_util::create_offline_player_uuid(&player_name(index));
        let folder = dir.join("world/players").join(folder);
        fs::create_dir_all(&folder).expect("player file folder");
        fs::write(folder.join(format!("{uuid}.json")), bytes).expect("player file");
    }
}

/// A server on an empty world holding `cases` as player files; every bot joins once.
fn run_with_bots(side: Side, dir: &Path, cases: Vec<(&'static str, &'static str, Vec<u8>)>) -> Run {
    let names = cases.len();
    let server = side
        .builder()
        .property("white-list", "false")
        .property("enforce-whitelist", "false")
        .dir(dir)
        .prepare(move |d| write_files(d, &cases))
        .start_unready()
        .expect("spawn the server");
    let started = server.wait_log(r"Done \(", 900).is_some();
    let exit = if started {
        ask(&server, "tick freeze");
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        for index in 0..names {
            // a player whose file makes the server refuse the join (`Invalid player data`) never gets as far as playing
            let bot = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(crate::common::join(
                    "127.0.0.1",
                    server.port,
                    &player_name(index),
                ))
            }));
            std::thread::sleep(Duration::from_secs(1));
            if let Ok(bot) = bot {
                runtime.block_on(bot.disconnect());
            }
        }
        ask(&server, "save-all flush");
        server.cmd("stop");
        server.wait_exit(Duration::from_secs(120)).map_or_else(
            || {
                server.kill();
                None
            },
            |s| s.code(),
        )
    } else {
        server
            .wait_exit(Duration::from_secs(120))
            .and_then(|s| s.code())
    };
    let raw_log = server.log();
    Run {
        dir: dir.to_owned(),
        exit,
        started,
        log: log_records(&raw_log),
        raw_log,
        source: None,
        below_zero: false,
    }
}

/// The log lines that name the player file of bot `index`, with the text that follows them, per bot.
fn per_bot(log: &[String], names: usize) -> Vec<Vec<String>> {
    let mut out = vec![Vec::new(); names];
    let mut current = None;
    for line in log {
        for index in 0..names {
            let uuid = mc_core::uuid_util::create_offline_player_uuid(&player_name(index));
            if line.contains(&uuid.to_string()) {
                current = Some(index);
            }
        }
        if let Some(index) = current {
            out[index].push(line.clone());
        }
    }
    out
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn damaged_player_files() {
    let cases: Vec<(&'static str, &'static str, Vec<u8>)> = cases()
        .into_iter()
        .map(|(folder, name, text)| {
            let bytes = invalid_utf8_bytes(name).unwrap_or_else(|| text.into_bytes());
            (folder, name, bytes)
        })
        .collect();
    let dirs = [
        tempfile::tempdir().expect("temp dir"),
        tempfile::tempdir().expect("temp dir"),
    ];
    let vanilla = run_with_bots(Side::Vanilla, dirs[0].path(), cases.clone());
    let ours = run_with_bots(Side::Ours, dirs[1].path(), cases.clone());
    let mut failures = Vec::new();
    if (vanilla.started, vanilla.exit) != (ours.started, ours.exit) {
        failures.push(format!(
            "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
            vanilla.started, vanilla.exit, ours.started, ours.exit
        ));
    }
    let (vanilla_log, ours_log) = (gate_log(&vanilla), gate_log(&ours));
    let (vanilla_by, ours_by) = (
        per_bot(&vanilla_log, cases.len()),
        per_bot(&ours_log, cases.len()),
    );
    for (index, (folder, name, _)) in cases.iter().enumerate() {
        match first_log_difference(&vanilla_by[index], &ours_by[index]) {
            None => println!("PLAYER FILE {folder}/{name}: equal"),
            Some(d) => failures.push(format!("== {folder}/{name}\n{d}")),
        }
    }
    if let Some(d) = first_log_difference(&vanilla_log, &ours_log) {
        failures.push(format!("log: {d}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The old-layout world the legacy structure file cases start from (`scripts/datafix-worlds.sh 1.12.2`).
fn old_world() -> std::path::PathBuf {
    let dir = cache_dir().join("worlds/1.12.2-normal");
    assert!(
        dir.join("level.dat").is_file(),
        "{}: no corpus world: run scripts/datafix-worlds.sh 1.12.2",
        dir.display()
    );
    dir
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn damaged_legacy_structure_files() {
    type Damage = fn(Vec<u8>) -> Vec<u8>;
    let cases: [(&str, Damage); 6] = [
        ("truncated", |b| b[..b.len() / 2].to_vec()),
        ("header_cut", |b| b[..12].to_vec()),
        ("three_bytes", |_| vec![1, 2, 3]),
        ("one_byte", |_| vec![0x1f]),
        ("zeroed_magic", |mut b| {
            b[0] = 0;
            b
        }),
        ("plain_nbt", |b| {
            let tag =
                mc_nbt::compress::read_compressed(&b, &mut mc_nbt::NbtAccounter::unlimited_heap())
                    .expect("a legacy structure file");
            mc_nbt::nbt_io::to_bytes(&tag)
        }),
    ];
    let mut failures = Vec::new();
    for (name, damage) in cases {
        let dirs = [
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        ];
        let runs: Vec<Run> = [Side::Vanilla, Side::Ours]
            .into_iter()
            .zip(&dirs)
            .map(|(side, dir)| {
                let world = old_world();
                run(
                    side,
                    dir.path(),
                    move |d| {
                        copy_dir(&world, &d.join("world"));
                        let _ = fs::remove_file(d.join("world/session.lock"));
                        let file = d.join("world/data/Mineshaft.dat");
                        let bytes = fs::read(&file).expect("the world has a Mineshaft.dat");
                        fs::write(&file, damage(bytes)).expect("damaged file");
                    },
                    &[],
                )
            })
            .collect();
        let mut findings = Vec::new();
        if (runs[0].started, runs[0].exit) != (runs[1].started, runs[1].exit) {
            findings.push(format!(
                "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
                runs[0].started, runs[0].exit, runs[1].started, runs[1].exit
            ));
        }
        if let Some(d) = first_log_difference(&gate_log(&runs[0]), &gate_log(&runs[1])) {
            findings.push(d);
        }
        println!(
            "LEGACY STRUCTURE FILE {name}: {}",
            if findings.is_empty() {
                "equal"
            } else {
                "differs"
            }
        );
        if !findings.is_empty() {
            failures.push(format!("== {name}\n{}", findings.join("\n")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
