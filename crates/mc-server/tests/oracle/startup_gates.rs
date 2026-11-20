//! Start-up gates: what the real 26.3 server and `mc-server` do with a world directory whose `level.dat` is of every age and
//! shape the start checks: a fresh start, the current version, the window below it, the worlds that need the file fixer,
//! a missing `DataVersion`, McRegion, an unknown storage version, another series, a newer world, and every way
//! `level.dat` can be corrupt (with and without `level.dat_old`).
//!
//! Each case compares the outcome (started, exit status), the warnings, errors and world-loading lines of the log in
//! order (with the first line of an exception that follows them, and the server directory printed as `<server>`), and the
//! files that are left (`level.dat` decoded, everything else by name and content; chunk files, `session.lock`,
//! `level.dat_old`, the clock fields and `level.dat_corrupted_*` are not compared: a corrupt file that was set aside is
//! counted instead; `game_rules.dat` is compared as a set of entries, the real server writes it in the order of an
//! identity-hashed map). The world data of the cases is cut from what the real server writes for a fresh world.
//! `scripts/datafix-world-dual.sh --cases gates` runs it.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use mc_nbt::compress::{gzip_compress, write_compressed};
use mc_nbt::{CompoundTag, Tag};
use worldcmp::tree::{self, TreeOptions, compare_trees};

use crate::world_start::{
    CLOCKS, GAME_RULES, INTEREST, Run, SERVER_FILES, Side, first_log_difference,
    game_rules_difference, ignores, run,
};

/// The level data a fresh start of the real server writes.
fn fresh_data() -> &'static CompoundTag {
    static DATA: LazyLock<CompoundTag> = LazyLock::new(|| {
        let dir = tempfile::tempdir().expect("temp dir");
        let fresh = run(Side::Vanilla, dir.path(), |_| {}, &[]);
        assert!(fresh.started, "the fresh start failed: {:?}", fresh.log);
        let bytes =
            fs::read(dir.path().join("world/level.dat")).expect("level.dat of a fresh world");
        mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap())
            .expect("level.dat")
            .get_compound("Data")
            .expect("Data")
            .clone()
    });
    &DATA
}

/// The fresh level data after `edit`.
fn data(edit: impl FnOnce(&mut CompoundTag)) -> CompoundTag {
    let mut data = fresh_data().clone();
    edit(&mut data);
    data
}

/// A level.dat with this `Data`.
fn level_dat(data: CompoundTag) -> Vec<u8> {
    let mut root = CompoundTag::new();
    root.put("Data", Tag::Compound(data));
    write_compressed(&root)
}

/// The level data of a world saved by data version `version` (`Version.Id` follows).
fn at(version: i32) -> CompoundTag {
    data(|d| {
        d.put_int("DataVersion", version);
        if let Some(v) = d.get_compound_mut("Version") {
            v.put_int("Id", version);
        }
    })
}

/// A gzip stream cut in half.
fn truncated() -> Vec<u8> {
    let bytes = level_dat(fresh_data().clone());
    bytes[..bytes.len() / 2].to_vec()
}

/// A gzip stream whose root tag is a list, not a compound.
fn list_root() -> Vec<u8> {
    gzip_compress(&[9, 0, 0, 0, 0, 0, 0, 0])
}

/// The files of the world folder of a case, `None` for a start without a world.
type Files = Option<Vec<(&'static str, Vec<u8>)>>;

fn cases() -> Vec<(&'static str, Files)> {
    let dat = |d| Some(vec![("level.dat", level_dat(d))]);
    let raw = |bytes: Vec<u8>| Some(vec![("level.dat", bytes)]);
    vec![
        ("fresh", None),
        ("current", dat(data(|_| {}))),
        ("window", dat(at(4950))),
        ("file_fix_4800", dat(at(4800))),
        ("file_fix_4000", dat(at(4000))),
        ("file_fix_1343", dat(at(1343))),
        ("file_fix_99", dat(at(99))),
        (
            "no_data_version",
            dat(data(|d| {
                d.remove("DataVersion");
                d.remove("Version");
            })),
        ),
        ("mcregion", dat(data(|d| d.put_int("version", 19132)))),
        (
            "mcregion_without_data_version",
            dat(data(|d| {
                d.put_int("version", 19132);
                d.remove("DataVersion");
                d.remove("Version");
            })),
        ),
        (
            "unknown_storage_version",
            dat(data(|d| d.put_int("version", 12345))),
        ),
        (
            "no_storage_version",
            dat(data(|d| {
                d.remove("version");
            })),
        ),
        (
            "other_series",
            dat(data(|d| {
                if let Some(v) = d.get_compound_mut("Version") {
                    v.put_string("Series", "other");
                }
            })),
        ),
        ("newer", dat(at(5100))),
        ("corrupt_empty", raw(Vec::new())),
        ("corrupt_truncated", raw(truncated())),
        ("corrupt_list_root", raw(list_root())),
        ("corrupt_no_data", raw(level_dat_root(CompoundTag::new()))),
        (
            "corrupt_data_version_string",
            dat(data(|d| d.put_string("DataVersion", "x"))),
        ),
        (
            "corrupt_both",
            Some(vec![
                ("level.dat", Vec::new()),
                ("level.dat_old", truncated()),
            ]),
        ),
        (
            "corrupt_level_dat_valid_old",
            Some(vec![
                ("level.dat", truncated()),
                ("level.dat_old", level_dat(fresh_data().clone())),
            ]),
        ),
        (
            "only_level_dat_old",
            Some(vec![("level.dat_old", level_dat(fresh_data().clone()))]),
        ),
    ]
}

/// A level.dat whose root is `root` itself (no `Data` entry added).
fn level_dat_root(root: CompoundTag) -> Vec<u8> {
    write_compressed(&root)
}

/// The warnings, errors and world-loading records of a run, each with the first line of the exception behind it.
fn gate_log(run: &Run) -> Vec<String> {
    let server = run.dir.to_string_lossy().into_owned();
    let mut out = Vec::new();
    let mut after_record = false;
    for line in run.raw_log.lines() {
        let record = line
            .starts_with('[')
            .then(|| line.split_once("]: "))
            .flatten();
        let Some((head, message)) = record else {
            // the first line of a stack trace that follows a kept record
            if after_record && !line.trim_start().starts_with("at ") && !line.is_empty() {
                out.push(format!("  {}", line.replace(&server, "<server>")));
            }
            after_record = false;
            continue;
        };
        let level = head
            .rsplit_once(" [")
            .map_or("", |(_, tl)| tl.rsplit('/').next().unwrap_or(""));
        if message.starts_with("Failed to request Minecraft Services public key") {
            after_record = false;
            continue;
        }
        let keep = matches!(level, "WARN" | "ERROR" | "FATAL")
            || INTEREST.iter().any(|i| message.starts_with(i))
            || message.starts_with("Attempting to use fallback");
        after_record = keep;
        if keep {
            out.push(format!("{level} {}", message.replace(&server, "<server>")));
        }
    }
    out
}

/// The `level.dat_corrupted_*` files set aside in the world folder.
fn set_aside(run: &Run) -> BTreeSet<String> {
    fs::read_dir(run.dir.join("world"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("level.dat_corrupted_"))
        .map(|_| "level.dat_corrupted_<time>".to_owned())
        .collect()
}

fn compare(vanilla: &Run, ours: &Run) -> Vec<String> {
    let mut findings = Vec::new();
    if (vanilla.started, vanilla.exit) != (ours.started, ours.exit) {
        findings.push(format!(
            "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
            vanilla.started, vanilla.exit, ours.started, ours.exit
        ));
    }
    if let Some(d) = first_log_difference(&gate_log(vanilla), &gate_log(ours)) {
        findings.push(d);
    }
    if set_aside(vanilla) != set_aside(ours) {
        findings.push(format!(
            "set aside: vanilla {:?}, ours {:?}",
            set_aside(vanilla),
            set_aside(ours)
        ));
    }
    let files = compare_trees(
        &vanilla.dir,
        &ours.dir,
        &TreeOptions {
            ignores: ignores(
                SERVER_FILES,
                &[CLOCKS, &["*/level.dat_corrupted_*", GAME_RULES]].concat(),
            ),
            chunks: None,
            max_listed: 10,
        },
    );
    if !files.is_clean() {
        findings.push(tree::render_text("vanilla", "ours", &files, 20));
    }
    if let Some(d) = game_rules_difference(&vanilla.dir, &ours.dir) {
        findings.push(d);
    }
    findings
}

fn write_world(dir: &Path, files: &Files) {
    let Some(files) = files else { return };
    let world = dir.join("world");
    fs::create_dir_all(&world).expect("world folder");
    for (name, bytes) in files {
        fs::write(world.join(name), bytes).expect("world file");
    }
}

/// Runs every case (or the ones named in `DATAFIX_GATE_CASES`, comma separated) and panics with the findings.
fn dual(selected: impl Fn(&str) -> bool) {
    let only: Option<Vec<String>> = std::env::var("DATAFIX_GATE_CASES")
        .ok()
        .map(|v| v.split(',').map(str::to_owned).collect());
    let mut failures = Vec::new();
    for (name, files) in cases() {
        if !selected(name) || only.as_ref().is_some_and(|o| !o.iter().any(|n| n == name)) {
            continue;
        }
        let dirs = [
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        ];
        let runs: Vec<Run> = [Side::Vanilla, Side::Ours]
            .into_iter()
            .zip(&dirs)
            .map(|(side, dir)| {
                let files = files.clone();
                run(side, dir.path(), move |d| write_world(d, &files), &[])
            })
            .collect();
        let findings = compare(&runs[0], &runs[1]);
        println!(
            "STARTUP GATE {name}: {}",
            if findings.is_empty() {
                "equal".to_owned()
            } else {
                format!("{} finding(s)", findings.len())
            }
        );
        if !findings.is_empty() {
            failures.push(format!("== {name}\n{}", findings.join("\n")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn start_up_gates() {
    dual(|_| true);
}
