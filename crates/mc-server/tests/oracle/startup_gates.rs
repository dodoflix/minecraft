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
use mc_testkit::{Corrupt, Deflate};
use worldcmp::tree::{self, TreeOptions, compare_trees};

use crate::world_start::{
    CLOCKS, GAME_RULES, INTEREST, Run, SERVER_FILES, Side, first_log_difference,
    game_rules_difference, ignores, run,
};

/// The level data and the saved data files (`name`, bytes) of a fresh world.
type FreshWorld = (CompoundTag, Vec<(String, Vec<u8>)>);

/// The level data and the saved data files (`data/...`) a fresh start of the real server writes.
fn fresh_world() -> &'static FreshWorld {
    static WORLD: LazyLock<FreshWorld> = LazyLock::new(|| {
        let dir = tempfile::tempdir().expect("temp dir");
        let fresh = run(Side::Vanilla, dir.path(), |_| {}, &[]);
        assert!(fresh.started, "the fresh start failed: {:?}", fresh.log);
        let world = dir.path().join("world");
        let bytes = fs::read(world.join("level.dat")).expect("level.dat of a fresh world");
        let data =
            mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap())
                .expect("level.dat")
                .get_compound("Data")
                .expect("Data")
                .clone();
        let mut saved = Vec::new();
        collect_dat(&world, &world, &mut saved);
        (data, saved)
    });
    &WORLD
}

/// The `.dat` files below `dir` (not `level.dat`), by path relative to `world`.
fn collect_dat(world: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("world folder")
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if path.starts_with(world.join("data")) {
                collect_dat(world, &path, out);
            }
        } else if path.starts_with(world.join("data"))
            && path.extension().is_some_and(|e| e == "dat")
        {
            let name = path.strip_prefix(world).expect("below world");
            out.push((
                name.to_string_lossy().into_owned(),
                fs::read(&path).expect("saved data file"),
            ));
        }
    }
}

/// The level data a fresh start of the real server writes.
fn fresh_data() -> &'static CompoundTag {
    &fresh_world().0
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

/// The same level data without the gzip layer.
fn plain_nbt() -> Vec<u8> {
    let mut root = CompoundTag::new();
    root.put("Data", Tag::Compound(fresh_data().clone()));
    mc_nbt::nbt_io::to_bytes(&root)
}

/// A level.dat whose first gzip magic byte is zero.
fn zeroed_magic() -> Vec<u8> {
    let mut bytes = level_dat(fresh_data().clone());
    bytes[0] = 0;
    bytes
}

/// A gzip stream of the type byte of a compound followed by a block the inflater rejects: the reader fails on the
/// stream itself, before the root tag has a name, so the exception is not wrapped by the tag loader.
fn corrupt_deflate(kind: Corrupt) -> Vec<u8> {
    Deflate::new().stored(&[10]).corrupt(kind).gzip()
}

/// A gzip stream of a root compound (`pad` zero bytes in a byte array, an int and a string) and `after` zero bytes behind
/// it, followed by a block the inflater rejects. The reader never looks behind the root compound, so whether the
/// failure is met depends on which refill of the stream the end of the compound falls in.
fn refill_window(pad: usize, after: usize) -> Vec<u8> {
    let mut root = CompoundTag::new();
    root.put("pad", Tag::ByteArray(vec![0; pad]));
    root.put_int("x", 1);
    root.put_string("name", "level");
    let bytes = mc_nbt::nbt_io::to_bytes(&root);
    let mut entry = vec![7u8, 0, 3, b'p', b'a', b'd'];
    entry.extend((pad as i32).to_be_bytes());
    let at = bytes
        .windows(entry.len())
        .position(|window| window == entry)
        .expect("the pad entry")
        + entry.len();
    let mut deflate = Deflate::new().stored(&bytes[..at]);
    if pad > 0 {
        deflate = deflate.zeros(pad);
    }
    deflate
        .stored(&bytes[at + pad..])
        .zeros(after)
        .corrupt(Corrupt::BlockType)
        .gzip()
}

/// A gzip stream whose root tag is a list, not a compound.
fn list_root() -> Vec<u8> {
    gzip_compress(&[9, 0, 0, 0, 0, 0, 0, 0])
}

/// The files of the world folder of a case, `None` for a start without a world.
type Files = Option<Vec<(String, Vec<u8>)>>;

fn cases() -> Vec<(&'static str, Files)> {
    let dat = |d| Some(vec![("level.dat".to_owned(), level_dat(d))]);
    let raw = |bytes: Vec<u8>| Some(vec![("level.dat".to_owned(), bytes)]);
    // a current world whose saved data files all went through `damage`
    let saved = |damage: fn(Vec<u8>) -> Vec<u8>| {
        let mut files = vec![("level.dat".to_owned(), level_dat(fresh_data().clone()))];
        files.extend(
            fresh_world()
                .1
                .iter()
                .map(|(name, bytes)| (name.clone(), damage(bytes.clone()))),
        );
        Some(files)
    };
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
                ("level.dat".to_owned(), Vec::new()),
                ("level.dat_old".to_owned(), truncated()),
            ]),
        ),
        ("corrupt_plain_nbt", raw(plain_nbt())),
        ("corrupt_zeroed_magic", raw(zeroed_magic())),
        ("corrupt_one_byte", raw(vec![0x1f])),
        ("corrupt_gzip_header_only", raw(vec![0x1f, 0x8b, 8])),
        (
            "corrupt_deflate_block_type",
            raw(corrupt_deflate(Corrupt::BlockType)),
        ),
        (
            "corrupt_deflate_stored_lengths",
            raw(corrupt_deflate(Corrupt::StoredLengths)),
        ),
        (
            "corrupt_deflate_distance_too_far",
            raw(corrupt_deflate(Corrupt::DistanceTooFar)),
        ),
        (
            "corrupt_deflate_literal_length_code",
            raw(corrupt_deflate(Corrupt::LiteralLengthCode)),
        ),
        (
            "corrupt_deflate_distance_code",
            raw(corrupt_deflate(Corrupt::DistanceCode)),
        ),
        (
            "corrupt_deflate_code_lengths_set",
            raw(corrupt_deflate(Corrupt::CodeLengthsSet)),
        ),
        (
            "corrupt_deflate_too_many_symbols",
            raw(corrupt_deflate(Corrupt::TooManySymbols)),
        ),
        ("refill_window_24540_1", raw(refill_window(24540, 1))),
        ("refill_window_21550_3000", raw(refill_window(21550, 3000))),
        ("refill_window_22000_3000", raw(refill_window(22000, 3000))),
        ("refill_window_24520_3000", raw(refill_window(24520, 3000))),
        ("refill_window_29750_3000", raw(refill_window(29750, 3000))),
        ("refill_window_10000_3000", raw(refill_window(10000, 3000))),
        ("refill_window_22200_3000", raw(refill_window(22200, 3000))),
        ("refill_window_5_9000", raw(refill_window(5, 9000))),
        (
            "corrupt_three_bytes_junk_old",
            Some(vec![
                ("level.dat".to_owned(), vec![1, 2, 3]),
                ("level.dat_old".to_owned(), vec![9; 40]),
            ]),
        ),
        (
            "corrupt_plain_nbt_truncated_old",
            Some(vec![
                ("level.dat".to_owned(), plain_nbt()),
                ("level.dat_old".to_owned(), truncated()),
            ]),
        ),
        ("saved_data_truncated", saved(|b| b[..b.len() / 2].to_vec())),
        ("saved_data_header_cut", saved(|b| b[..12].to_vec())),
        ("saved_data_three_bytes", saved(|_| vec![1, 2, 3])),
        ("saved_data_one_byte", saved(|_| vec![0x1f])),
        (
            "saved_data_zeroed_magic",
            saved(|mut b| {
                b[0] = 0;
                b
            }),
        ),
        (
            "saved_data_plain_nbt",
            saved(|b| {
                let tag = mc_nbt::compress::read_compressed(
                    &b,
                    &mut mc_nbt::NbtAccounter::unlimited_heap(),
                )
                .expect("a saved data file");
                mc_nbt::nbt_io::to_bytes(&tag)
            }),
        ),
        (
            "corrupt_level_dat_valid_old",
            Some(vec![
                ("level.dat".to_owned(), truncated()),
                ("level.dat_old".to_owned(), level_dat(fresh_data().clone())),
            ]),
        ),
        (
            "only_level_dat_old",
            Some(vec![(
                "level.dat_old".to_owned(),
                level_dat(fresh_data().clone()),
            )]),
        ),
    ]
}

/// A level.dat whose root is `root` itself (no `Data` entry added).
fn level_dat_root(root: CompoundTag) -> Vec<u8> {
    write_compressed(&root)
}

/// The warnings, errors and world-loading records of a run, each with the first line of the exception behind it.
pub(crate) fn gate_log(run: &Run) -> Vec<String> {
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
        // the request to the Mojang services fails with the network and its WARN lands at any record of the start (vanilla:
        // `Failed to connect`, ours: the same text plus `Services are unavailable...`), whichever side's request ends first
        if message.starts_with("Failed to request Minecraft Services public key")
            || head.ends_with("[Minecraft Services Discovery/WARN")
        {
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

/// `compare_files` is off for the cases that leave a damaged file the tree comparison cannot decode.
fn compare(vanilla: &Run, ours: &Run, compare_files: bool) -> Vec<String> {
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
    if !compare_files {
        return findings;
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
        let path = world.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("world subfolder");
        fs::write(path, bytes).expect("world file");
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
        let findings = compare(&runs[0], &runs[1], !name.starts_with("saved_data"));
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

/// The cases whose gzip stream the inflater rejects after a valid start.
fn is_inflater_case(name: &str) -> bool {
    name.starts_with("corrupt_deflate_") || name.starts_with("refill_window_")
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn start_up_gates() {
    dual(|name| !is_inflater_case(name));
}

/// Streams that break in the middle of the deflate data: zlib's message for each kind of damage, and which refill the
/// failure is met in.
#[test]
#[ignore = "oracle: spawns vanilla server"]
fn start_up_inflater_failures() {
    dual(is_inflater_case);
}
