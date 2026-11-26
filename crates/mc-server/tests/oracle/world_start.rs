//! World start dual: the real 26.3 server and `mc-server` open identical copies of a world that an older vanilla
//! server generated (`scripts/datafix-worlds.sh`), save and stop, and the two server directories are compared.
//!
//! Cases: a normal start, and a world newer than the current data version (what the current vanilla server made of the
//! corpus world, with every data version raised). The worlds come from `$MC_DATAFIX_CACHE/worlds` (`<id>`, and
//! `<id>-normal` with `DATAFIX_DUAL_KINDS=flat,normal`); `DATAFIX_DUAL_WORLDS` selects them: `all` (default: every
//! version of `versions.tsv`) or a list of ids. `WORLD_UPGRADE_KEEP=<dir>` keeps both server
//! directories of a run under `<dir>/<case>/<world>/{vanilla,ours}`. `scripts/datafix-world-dual.sh` runs the matrix.
//!
//! What is compared, and what is normalised (everything else is exact: tag types, key order, serialised bytes):
//! * The files of the server directory, except `logs/`, the unpacked server libraries and `cache/`, `stderr.log`, the
//!   jar link, `eula.txt` and `server.properties` (each server rewrites the last one with its own date line)
//!   ([`SERVER_FILES`]). `*.mca` files are compared by chunk instead of by byte: outside [`LOADED`] chunks of the
//!   spawn and the force-loaded area exactly, with `worldcmp --exact`; inside it semantically (`worldcmp` default mode,
//!   chunks of status full on both sides: `LastUpdate`, `InhabitedTime` and light are not compared), because the
//!   server rewrites a loaded chunk with runtime fields and the saved result depends on when the save ran. The POI files
//!   of the loaded chunks are still exact, and so are the chunks below status full inside the rectangle, except for
//!   their light layers ([`PARTIAL_CHUNK_LIGHT`]) and, for a chunk the servers generated while running (the world
//!   does not store it), the fields two runs of the real server disagree on ([`GENERATED_CHUNK_RACES`]). In the overworld
//!   of a normal world older than 1.18 the sections, ticks and block entities below y = 0 and the post processing
//!   marks are left out of every comparison ([`BELOW_ZERO_RACES`]).
//! * Random ticks are off in both copies of the world (`random_tick_speed` 0): they draw from an unseeded random source.
//! * Tick-driven values ([`CLOCKS`]): the two runs do not execute the same number of ticks between `Done` and the
//!   freeze. `LastPlayed` is wall-clock time. `session.lock` and `level.dat_old` are not compared (a timestamp, and the
//!   copy of the previous `level.dat`).
//! * `game_rules.dat` as a set of entries: the real server writes the rules in the order of an identity-hashed map, so even
//!   two runs of it disagree on the key order of the file ([`game_rules_difference`]).
//! * The log: warnings, errors and the lines of [`INTEREST`] in order, the time and the thread removed, stack traces
//!   dropped (the lines of a `Serialization errors:` report kept), the failed request for the services' public keys and
//!   the discovery thread's warning left out; a run of `Failed to fetch mob spawner entity` errors (logged from
//!   concurrent worker threads) is sorted ([`log_records`]).
//! * A world on which the two servers differ is run again on the real server, up to [`VANILLA_RETRIES`] more times
//!   (`mc-server` runs once): its first tick is a race.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::NbtAccounter;
use mc_nbt::compress::{read_compressed, write_compressed};
use mc_testkit::{Builder, Server};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use worldcmp::compare::status_rank;
use worldcmp::fields::{FieldSpec, Ignores};
use worldcmp::tree::{self, TreeOptions, compare_trees};
use worldcmp::world::{Filter, compare_dimension, dimension_dir, render_text};

use crate::common::world_diff::{read_dat, read_region};
use crate::entity_persistence::ask;

/// The data version `bump_data_versions` writes: above the current version, same series.
const NEWER: i32 = 5100;

/// Chunks around (0, 0) that are loaded after the start: the spawn area and the area the scripts force-load.
const LOADED: i32 = 8;

/// The first data version that has the world height of 1.18 (`has_increased_height_already`): older worlds stop at y = 0.
const NORMAL_WORLD_BELOW_ZERO: i32 = 2825;

/// Entries of the server directory that are not part of the comparison (the output of the server process itself).
pub(crate) const SERVER_FILES: &[&str] = &[
    "logs/",
    "libraries/",
    "versions/",
    "cache/",
    "stderr.log",
    "vanilla-under-test.jar",
    "eula.txt",
    "server.properties",
    "**/*.mca",
    "*/session.lock",
    "*/level.dat_old",
];

/// Wall-clock and tick-driven values (the runs start at different times and execute a different number of ticks).
pub(crate) const CLOCKS: &[&str] = &[
    "*/level.dat#Data.LastPlayed",
    "*/level.dat#Data.Time",
    "*/data/minecraft/weather.dat#data.rain_time",
    "*/data/minecraft/weather.dat#data.thunder_time",
    "*/data/minecraft/world_clocks.dat#data.*.total_ticks",
    "*/dimensions/*/*/data/minecraft/raids.dat#data.tick",
];

/// The light layers of a chunk below status full: what the asynchronous light engine had propagated into it when the
/// server saved it differs from run to run (two runs of the real server save different sections).
const PARTIAL_CHUNK_LIGHT: &[&str] = &[
    "region/*.mca#sections.*.BlockLight",
    "region/*.mca#sections.*.SkyLight",
    "**/region/*.mca#sections.*.BlockLight",
    "**/region/*.mca#sections.*.SkyLight",
];

/// The fields of a chunk that the servers generated while running (the world has no such chunk) that two runs of the
/// real server on the same world disagree on: which block a feature of a neighbour chunk places first depends on the
/// order the worker threads finish in. Seen between two runs of the real server on the 1.16.5 and 1.17.1 corpus worlds:
/// the blocks of a section (data and palette order, the age of vines), the fluid tick queue (nether), the post
/// processing marks (overworld) and the UUID of an end crystal (end).
const GENERATED_CHUNK_RACES: &[&str] = &[
    "region/*.mca#sections.*.block_states",
    "**/region/*.mca#sections.*.block_states",
    "region/*.mca#fluid_ticks",
    "**/region/*.mca#fluid_ticks",
    "region/*.mca#entities.*.UUID",
    "**/region/*.mca#entities.*.UUID",
    "region/*.mca#PostProcessing",
    "**/region/*.mca#PostProcessing",
];

/// The overworld below y = 0 of a normal world a version before 1.18 generated. Such a world has no blocks there; the
/// servers generate them when they load a chunk (the lush and deep caves of the current version), and two runs of the
/// real server on the same world disagree on them in about half of the 17 x 17 chunks around the spawn: the blocks of the
/// sections below 0 (placement order of overlapping cave features), their light, the scheduled ticks and block entities
/// below 0, and the post processing marks. Evidence, two runs of the real server on the same copy of the world: on the
/// 1.14.4 normal world 103 of the 169 full chunks of the window differ, with 11619 block differences of which all but
/// five (diorite/granite/andesite at y 13 to 16 of chunk [-6, 6]) lie at y < 0; four further runs of that world and of the
/// 1.16.5 and 1.17.1 normal worlds show the same (50 or more differing chunks per pair, none of them in two runs of
/// `mc-server`).
/// The part of these chunks at y >= 0 stays compared, except `PostProcessing` (one list per section, left out whole), and a
/// file the specs touch is not compared by its decompressed bytes. Only for `World::below_zero` worlds, in the overworld.
const BELOW_ZERO_RACES: &[&str] = &[
    "dimensions/minecraft/overworld/region/*.mca#sections.*[Y<0]",
    "dimensions/minecraft/overworld/region/*.mca#block_ticks.*[y<0]",
    "dimensions/minecraft/overworld/region/*.mca#fluid_ticks.*[y<0]",
    "dimensions/minecraft/overworld/region/*.mca#block_entities.*[y<0]",
    "dimensions/minecraft/overworld/region/*.mca#PostProcessing",
];

/// Chunks in which two runs of the real server place overlapping feature blocks differently at the lowest sections: the
/// features of the neighbour chunks that decorate them finish on worker threads in an order that varies. The sections
/// below y = 32 of such a chunk are left out of the semantic comparison, the rest of it and every other chunk stay
/// compared. `(dimension, chunk, only worlds older than 1.18)`. Evidence, runs of the real server and of `mc-server` on
/// the same copy of each normal world:
/// * overworld [-6, 6] (diorite, granite and andesite blobs at y 13 to 16): the 1.14.4 world, 5 blocks in two of three
///   pairs of real runs; the 1.16.5 world, 16 blocks in one of three pairs; the 1.17.1 world, 16 blocks in two of three
///   pairs. The only block difference at y >= 0 that remained between the servers in 9 runs lies in this chunk.
/// * nether [6, -5] (8 blocks) and [6, -4] (3 blocks) of the 1.17.1 world: gravel against magma block at y 28 to 30 in
///   two of three pairs of real runs, nothing else differs between two real runs there.
///
/// The runs of `mc-server` are identical in these chunks in every pair (nine pairs).
const BLOCK_RACE_CHUNKS: &[(&str, (i32, i32), bool)] = &[
    ("overworld", (-6, 6), true),
    ("the_nether", (6, -5), false),
    ("the_nether", (6, -4), false),
];

/// `rect` without the chunk `hole` (at most four rectangles).
fn rect_without(
    rect @ (x1, z1, x2, z2): (i32, i32, i32, i32),
    (hx, hz): (i32, i32),
) -> Vec<(i32, i32, i32, i32)> {
    if !(x1..=x2).contains(&hx) || !(z1..=z2).contains(&hz) {
        return vec![rect];
    }
    [
        (x1, z1, x2, hz - 1),
        (x1, hz + 1, x2, z2),
        (x1, hz, hx - 1, hz),
        (hx + 1, hz, x2, hz),
    ]
    .into_iter()
    .filter(|(a, b, c, d)| a <= c && b <= d)
    .collect()
}

/// Reruns of the real server on a world that differs from `mc-server` (never more than this many; `mc-server` runs once and
/// has to be deterministic). The real server's first tick is a race: after `Done` it entity-ticks the loaded chunks whose
/// promotion the start-up wait had finished, and it ran that promotion for chunk [0, 0] of the old flat corpus worlds in
/// most runs (1.14.4 flat, six runs: four with the tick, two without) and not in the others. `mc-server` finishes the
/// promotions before `Done`, which is what the majority of the real runs show.
pub(crate) const VANILLA_RETRIES: usize = 3;

/// Info lines that belong to the world loading and the file fixer (warnings and errors are always compared).
pub(crate) const INTEREST: &[&str] = &[
    "Starting upgrade for world",
    "Moving out old world folder",
    "Moving in new world folder",
    "Upgrade done for world",
    "No existing world data",
    "This world",
    "Loaded ",
];

const PROPERTIES: [&str; 9] = [
    // a world that is created has the same seed on both sides
    "level-seed=12345",
    "online-mode=false",
    "enable-rcon=false",
    "enable-query=false",
    "max-tick-time=-1",
    "rate-limit=0",
    "spawn-protection=0",
    "view-distance=4",
    "simulation-distance=4",
];

#[derive(Clone, Copy, Debug)]
enum Case {
    Normal,
    Newer,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Case::Normal => "normal",
            Case::Newer => "newer",
        }
    }
}

/// The arguments with which the current vanilla server brings every chunk of a world to the current format.
const FORCE_UPGRADE: &[&str] = &["--forceUpgrade"];

pub(crate) struct World {
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
    /// A normal world of a version before 1.18: the overworld has no blocks below y = 0 and the servers generate them
    /// while loading ([`BELOW_ZERO_RACES`]).
    pub(crate) below_zero: bool,
}

pub(crate) fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MC_DATAFIX_CACHE") {
        return dir.into();
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .expect("neither MC_DATAFIX_CACHE, XDG_CACHE_HOME nor HOME is set");
    base.join("mc-datafix-corpus")
}

/// `(id, DataVersion)` of `versions.tsv` (`-1`: the world has none).
fn versions() -> Vec<(String, i32)> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../mc-datafix/tests/fixtures/versions.tsv");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(1)
        .map(|l| {
            let mut f = l.split('\t');
            let id = f.next().expect("id").to_owned();
            let version = f
                .next()
                .expect("data version")
                .parse()
                .expect("data version");
            (id, version)
        })
        .collect()
}

/// The worlds of this run (see the module documentation); a missing world directory fails with the command that makes it.
pub(crate) fn worlds() -> Vec<World> {
    let all = versions();
    let selection = std::env::var("DATAFIX_DUAL_WORLDS").unwrap_or_else(|_| "all".into());
    let ids: Vec<String> = match selection.as_str() {
        "all" => all.iter().map(|(id, _)| id.clone()).collect(),
        list => list.split(',').map(str::to_owned).collect(),
    };
    let kinds = std::env::var("DATAFIX_DUAL_KINDS").unwrap_or_else(|_| "flat".into());
    let mut out = Vec::new();
    for id in ids {
        let version = all.iter().find(|(i, _)| *i == id).map_or(-1, |(_, v)| *v);
        for kind in kinds.split(',') {
            let name = if kind == "normal" {
                format!("{id}-normal")
            } else {
                id.clone()
            };
            let dir = cache_dir().join("worlds").join(&name);
            assert!(
                dir.join("level.dat").is_file(),
                "{}: no corpus world: run scripts/datafix-worlds.sh {id}",
                dir.display()
            );
            let below_zero = kind == "normal" && version < NORMAL_WORLD_BELOW_ZERO;
            out.push(World {
                name,
                dir,
                below_zero,
            });
        }
    }
    assert!(
        !out.is_empty(),
        "no world selected (DATAFIX_DUAL_WORLDS={selection})"
    );
    out
}

pub(crate) fn copy_dir(from: &Path, to: &Path) {
    let status = Command::new("cp")
        .arg("-r")
        .arg(from)
        .arg(to)
        .status()
        .expect("cp");
    assert!(status.success(), "cp {} {}", from.display(), to.display());
}

/// Gives `level.dat`, every saved data file and every chunk of `region/`, `entities/` and `poi/` below `world` `version` as data version.
fn bump_data_versions(world: &Path, version: i32) {
    let level = world.join("level.dat");
    let bytes = fs::read(&level).expect("level.dat");
    let mut tag = read_compressed(&bytes, &mut NbtAccounter::unlimited_heap()).expect("level.dat");
    let data = tag.get_compound_mut("Data").expect("Data");
    data.put_int("DataVersion", version);
    if let Some(v) = data.get_compound_mut("Version") {
        v.put_int("Id", version);
    }
    fs::write(&level, write_compressed(&tag)).expect("write level.dat");
    bump_saved_data(world, version);
    let mut folders = Vec::new();
    collect_chunk_folders(world, &mut folders);
    for folder in folders {
        let info = RegionStorageInfo::new("t", Identifier::parse("overworld").unwrap(), "chunk");
        let mut storage = RegionFileStorage::new(info, &folder, false);
        for entry in fs::read_dir(&folder).expect("chunk folder").flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut parts = name.strip_suffix(".mca").unwrap_or("").split('.').skip(1);
            let (Some(Ok(rx)), Some(Ok(rz))) = (
                parts.next().map(str::parse::<i32>),
                parts.next().map(str::parse::<i32>),
            ) else {
                continue;
            };
            for pos in (0..1024).map(|i| ChunkPos::new(rx * 32 + i % 32, rz * 32 + i / 32)) {
                if let Some(mut chunk) = storage.read(&pos).expect("read chunk") {
                    chunk.put_int("DataVersion", version);
                    storage.write(&pos, Some(&chunk)).expect("write chunk");
                }
            }
        }
        storage.close().expect("close region storage");
    }
}

/// The `DataVersion` of the saved data files (`data/**/*.dat`, `players/**`). `data/minecraft/maps/last_id.dat` is written
/// uncompressed and has no `DataVersion`: it stays as it is.
fn bump_saved_data(dir: &Path, version: i32) {
    for entry in fs::read_dir(dir).expect("world folder").flatten() {
        let path = entry.path();
        if path.is_dir() {
            bump_saved_data(&path, version);
        } else if path.extension().is_some_and(|e| e == "dat")
            && path.file_name().is_some_and(|n| n != "level.dat")
        {
            let bytes = fs::read(&path).expect("saved data");
            if !bytes.starts_with(&[0x1f, 0x8b]) {
                continue;
            }
            let mut tag = read_compressed(&bytes, &mut NbtAccounter::unlimited_heap())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            if tag.get_int("DataVersion").is_some() {
                tag.put_int("DataVersion", version);
                fs::write(&path, write_compressed(&tag)).expect("write saved data");
            }
        }
    }
}

fn collect_chunk_folders(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("world folder").flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match path.file_name().and_then(|n| n.to_str()) {
            Some("region" | "entities" | "poi") => out.push(path),
            _ => collect_chunk_folders(&path, out),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Side {
    Vanilla,
    Ours,
}

impl Side {
    pub(crate) fn builder(self) -> Builder {
        match self {
            Side::Vanilla => Server::vanilla(),
            Side::Ours => Server::ours(env!("CARGO_BIN_EXE_mc-server")),
        }
        // the zone is part of how dates print
        .env("TZ", "UTC")
        .properties(PROPERTIES)
    }
}

/// What one server did in its directory.
pub(crate) struct Run {
    pub(crate) dir: PathBuf,
    /// The exit code, `None` when the server had to be killed.
    pub(crate) exit: Option<i32>,
    /// Whether the server got as far as accepting players.
    pub(crate) started: bool,
    pub(crate) log: Vec<String>,
    /// The whole output of the server.
    pub(crate) raw_log: String,
    /// The world the server was started on, when it was copied from one ([`run_on`]).
    pub(crate) source: Option<PathBuf>,
    /// The overworld below y = 0 of the world is generated while the server loads it ([`World::below_zero`]).
    pub(crate) below_zero: bool,
}

/// A server in `dir` (which `prepare` fills before the start) with the arguments `args`: when it starts, it freezes,
/// saves and stops; when it ends by itself (a refused world, a failed upgrade) that is the outcome.
pub(crate) fn run(
    side: Side,
    dir: &Path,
    prepare: impl FnOnce(&Path) + 'static,
    args: &[&str],
) -> Run {
    run_limited(side, dir, prepare, args, None)
}

/// [`run`] with the size of the files the server may write capped at `file_size_limit` bytes.
pub(crate) fn run_limited(
    side: Side,
    dir: &Path,
    prepare: impl FnOnce(&Path) + 'static,
    args: &[&str],
    file_size_limit: Option<u64>,
) -> Run {
    let mut builder = side.builder().dir(dir).prepare(prepare);
    if let Some(bytes) = file_size_limit {
        builder = builder.file_size_limit(bytes);
    }
    for arg in args {
        builder = builder.arg(arg);
    }
    let server = builder.start_unready().expect("spawn the server");
    let started = server.wait_log(r"Done \(", 900).is_some();
    let exit = if started {
        ask(&server, "tick freeze");
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
    let log = log_records(&raw_log);
    Run {
        dir: dir.to_owned(),
        exit,
        started,
        log,
        raw_log,
        source: None,
        below_zero: false,
    }
}

/// A server on a copy of `world` (the directory `world` of the server directory).
fn run_on(side: Side, dir: &Path, world: &Path, args: &[&str]) -> Run {
    let source = world.to_owned();
    let copied = source.clone();
    Run {
        source: Some(source),
        ..run(
            side,
            dir,
            move |dir| copy_dir(&copied, &dir.join("world")),
            args,
        )
    }
}

/// The records of `log` that are compared: `LEVEL message`, in order (see the module documentation).
pub(crate) fn log_records(log: &str) -> Vec<String> {
    let mut out = Vec::new();
    // the lines under a `Serialization errors:` record are its report (the paths and texts of the problems)
    let mut report = false;
    for line in log.lines() {
        if !line.starts_with('[') {
            if report && let Some(record) = out.last_mut() {
                *record = format!("{record}\n{line}");
            }
            continue;
        }
        // `[12:00:00] [Server thread/INFO]: message`; every other line is a continuation of the previous record.
        report = false;
        let Some((head, message)) = line.split_once("]: ") else {
            continue;
        };
        let Some((_, thread_level)) = head.rsplit_once(" [") else {
            continue;
        };
        let level = thread_level.rsplit('/').next().unwrap_or("");
        // a request to the Mojang services: it fails (or not) with the network, and with a different text in each server.
        // The discovery thread logs the failure of its request whenever the request ends, so the WARN lands at any record
        // of the start: runs of ours on the same world logged it at record 9 where the real server's run logged none (the
        // real server logs it too: a run of it under a debugger did)
        if message.starts_with("Failed to request Minecraft Services public key")
            || thread_level == "Minecraft Services Discovery/WARN"
        {
            continue;
        }
        let interesting = matches!(level, "WARN" | "ERROR" | "FATAL")
            || INTEREST.iter().any(|i| message.starts_with(i));
        if interesting {
            out.push(format!("{level} {message}"));
        }
        report = interesting && message.ends_with("Serialization errors:");
    }
    // Two rooms of the 1.14.4 normal world fail to fetch their spawner, each on its own worker thread: the real server
    // logged them in the order (50, 38, 67), (-59, 31, 37) in one run and in the reverse order in another, `mc-server`
    // logs both positions too. A run of such records is compared sorted.
    let mut start = 0;
    while start < out.len() {
        let run = out[start..]
            .iter()
            .take_while(|r| r.starts_with("ERROR Failed to fetch mob spawner entity"))
            .count();
        out[start..start + run].sort();
        start += run.max(1);
    }
    out
}

pub(crate) fn first_log_difference(vanilla: &[String], ours: &[String]) -> Option<String> {
    let n = vanilla.len().max(ours.len());
    (0..n).find_map(|i| {
        let (v, o) = (vanilla.get(i), ours.get(i));
        (v != o).then(|| {
            format!(
                "log record {i}:\n  vanilla: {}\n  ours:    {}",
                v.map_or("(end)", String::as_str),
                o.map_or("(end)", String::as_str)
            )
        })
    })
}

pub(crate) fn ignores(patterns: &[&str], extra: &[&str]) -> Ignores {
    Ignores(
        patterns
            .iter()
            .chain(extra)
            .map(|p| p.parse::<FieldSpec>().expect("ignore spec"))
            .collect(),
    )
}

/// `game_rules.dat`, compared by [`game_rules_difference`] and not by the tree comparison.
pub(crate) const GAME_RULES: &str = "*/data/minecraft/game_rules.dat";

/// `game_rules.dat` as sorted `key = value` lines (`None` when absent): the real server writes the rules in the order of
/// an identity-hashed map, so even two runs of it disagree on the key order of the file.
pub(crate) fn game_rules_difference(vanilla: &Path, ours: &Path) -> Option<String> {
    let rules = |dir: &Path| -> Option<Vec<String>> {
        let file = dir.join("world/data/minecraft/game_rules.dat");
        file.is_file().then(|| {
            let tag = read_dat(&file);
            let mut lines: Vec<String> = tag
                .keys()
                .filter(|k| *k != "data")
                .map(|k| format!("{} = {:?}", k.as_lossy_str(), tag.get_java(k)))
                .collect();
            let data = tag.get_compound_or_empty("data");
            lines.extend(
                data.keys()
                    .map(|k| format!("data.{} = {:?}", k.as_lossy_str(), data.get_java(k))),
            );
            lines.sort();
            lines
        })
    };
    let (v, o) = (rules(vanilla), rules(ours));
    (v != o).then(|| {
        let only = |a: &Option<Vec<String>>, b: &Option<Vec<String>>| {
            a.as_ref().map_or_else(
                || "absent".to_owned(),
                |a| {
                    let missing: Vec<&String> = a
                        .iter()
                        .filter(|l| !b.as_ref().is_some_and(|b| b.contains(l)))
                        .collect();
                    format!("{missing:?}")
                },
            )
        };
        format!(
            "game_rules.dat (compared as a set): only vanilla {}, only ours {}",
            only(&v, &o),
            only(&o, &v)
        )
    })
}

/// The positions of the chunks that `world` stores in `dim`, whatever their status.
fn stored_chunks(world: &Path, dim: &str) -> BTreeSet<(i32, i32)> {
    let Ok(files) = fs::read_dir(dimension_dir(world, dim).1.join("region")) else {
        return BTreeSet::new();
    };
    files
        .flatten()
        .map(|f| f.path())
        .filter(|f| f.extension().is_some_and(|e| e == "mca"))
        .flat_map(|file| read_region(&file).into_keys())
        .collect()
}

/// The chunks inside `rect` whose status is below full in either world (the semantic mode skips them).
fn chunks_below_full(
    vanilla: &Path,
    ours: &Path,
    dim: &str,
    (x1, z1, x2, z2): (i32, i32, i32, i32),
) -> BTreeSet<(i32, i32)> {
    let full = status_rank("full").expect("status");
    let mut out = BTreeSet::new();
    for world in [vanilla, ours] {
        let folder = dimension_dir(world, dim).1.join("region");
        let Ok(files) = fs::read_dir(&folder) else {
            continue;
        };
        for file in files.flatten().map(|f| f.path()) {
            if file.extension().is_none_or(|e| e != "mca") {
                continue;
            }
            for ((x, z), chunk) in read_region(&file) {
                let status = status_rank(chunk.get_string_or("Status", "")).unwrap_or(0);
                if status < full && (x1..=x2).contains(&x) && (z1..=z2).contains(&z) {
                    out.insert((x, z));
                }
            }
        }
    }
    out
}

/// Differences between two finished runs, as printable findings (empty: equal under the normalisation above).
pub(crate) fn compare(vanilla: &Run, ours: &Run) -> Vec<String> {
    let mut findings = Vec::new();
    if (vanilla.started, vanilla.exit) != (ours.started, ours.exit) {
        findings.push(format!(
            "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
            vanilla.started, vanilla.exit, ours.started, ours.exit
        ));
    }
    if let Some(d) = first_log_difference(&vanilla.log, &ours.log) {
        findings.push(d);
    }
    let files = compare_trees(
        &vanilla.dir,
        &ours.dir,
        &TreeOptions {
            ignores: ignores(SERVER_FILES, &[CLOCKS, &[GAME_RULES]].concat()),
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
    let (v, o) = (vanilla.dir.join("world"), ours.dir.join("world"));
    if !v.is_dir() || !o.is_dir() {
        return findings;
    }
    let exact = |chunks, skip: &[&str]| Filter {
        chunks: Some(chunks),
        min_status: 0,
        max_listed: 10,
        ignore: Vec::new(),
        exact: true,
        ignores: ignores(skip, &[]),
    };
    const FAR: i32 = 1 << 20;
    let outside = [
        (-FAR, -FAR, -LOADED - 1, FAR),
        (LOADED + 1, -FAR, FAR, FAR),
        (-LOADED, -FAR, LOADED, -LOADED - 1),
        (-LOADED, LOADED + 1, LOADED, FAR),
    ];
    let inner = (-LOADED, -LOADED, LOADED, LOADED);
    let semantic = |skip: &[&str]| Filter {
        chunks: Some(inner),
        min_status: status_rank("full").expect("status"),
        max_listed: 10,
        ignore: Vec::new(),
        exact: false,
        ignores: ignores(skip, &[]),
    };
    let mut reports = Vec::new();
    for dim in ["overworld", "the_nether", "the_end"] {
        let below_zero = if dim == "overworld" && vanilla.below_zero {
            BELOW_ZERO_RACES
        } else {
            &[]
        };
        for rect in outside {
            reports.push(compare_dimension(&v, &o, dim, &exact(rect, below_zero)));
        }
        // the chunks of [`BLOCK_RACE_CHUNKS`] are compared on their own, without the sections that hold the races
        let holes: Vec<(i32, i32)> = BLOCK_RACE_CHUNKS
            .iter()
            .filter(|(d, _, old_only)| *d == dim && (!old_only || vanilla.below_zero))
            .map(|(_, chunk, _)| *chunk)
            .collect();
        let rects = holes.iter().fold(vec![inner], |rects, hole| {
            rects
                .into_iter()
                .flat_map(|r| rect_without(r, *hole))
                .collect()
        });
        for rect in rects {
            let filter = Filter {
                chunks: Some(rect),
                ..semantic(below_zero)
            };
            reports.push(compare_dimension(&v, &o, dim, &filter));
        }
        let race = format!("dimensions/minecraft/{dim}/region/*.mca#sections.*[Y<2]");
        for (x, z) in holes {
            let filter = Filter {
                chunks: Some((x, z, x, z)),
                ..semantic(&[below_zero, &[race.as_str()][..]].concat())
            };
            reports.push(compare_dimension(&v, &o, dim, &filter));
        }
        // the semantic mode skips chunks below status full: those are compared exactly, one by one; the chunks the
        // servers generated while running (the world has no such chunk) also leave out [`GENERATED_CHUNK_RACES`]
        let stored = vanilla
            .source
            .as_deref()
            .map(|source| stored_chunks(source, dim));
        for (x, z) in chunks_below_full(&v, &o, dim, inner) {
            let generated = stored.as_ref().is_some_and(|s| !s.contains(&(x, z)));
            let races = if generated {
                GENERATED_CHUNK_RACES
            } else {
                &[]
            };
            let skip = [PARTIAL_CHUNK_LIGHT, races, below_zero].concat();
            reports.push(compare_dimension(&v, &o, dim, &exact((x, z, x, z), &skip)));
        }
        // the POI files of the loaded chunks are still exact (the exact mode with the other storages left out)
        let chunk_storages = ["region/", "**/region/", "entities/", "**/entities/"];
        reports.push(compare_dimension(
            &v,
            &o,
            dim,
            &exact(inner, &chunk_storages),
        ));
    }
    if reports.iter().any(|r| !r.is_clean()) {
        let dirty: Vec<_> = reports.into_iter().filter(|r| !r.is_clean()).collect();
        findings.push(render_text("vanilla", "ours", &dirty, 20));
    }
    findings
}

fn keep(case: Case, world: &World, vanilla: &Run, ours: &Run) {
    let Some(root) = std::env::var_os("WORLD_UPGRADE_KEEP") else {
        return;
    };
    let base = PathBuf::from(root).join(case.name()).join(&world.name);
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("keep directory");
    copy_dir(&vanilla.dir, &base.join("vanilla"));
    copy_dir(&ours.dir, &base.join("ours"));
}

/// The world both servers start from: a copy of the corpus world, or for [`Case::Newer`] what the current vanilla server
/// makes of it with `--forceUpgrade` (every chunk in the current format) with the data version of level.dat, the saved
/// data and every chunk raised to [`NEWER`]. Random ticks are switched off in it ([`stop_random_ticks`]).
fn source_world(case: Case, world: &World, staging: &Path) -> PathBuf {
    let copy = staging.join("world");
    if matches!(case, Case::Newer) {
        let upgraded = run_on(Side::Vanilla, staging, &world.dir, FORCE_UPGRADE);
        assert!(
            upgraded.started,
            "staging the world failed: {:?}",
            upgraded.log
        );
        bump_data_versions(&copy, NEWER);
    } else {
        copy_dir(&world.dir, &copy);
    }
    stop_random_ticks(&copy);
    copy
}

/// `random_tick_speed` 0 in the game rules of `world` (the saved data file, or `level.dat` of a world that has none yet):
/// the random ticks of a loaded chunk (vine growth, leaf decay) draw from an unseeded random source in both servers, and
/// the two runs do not execute the same number of ticks before the freeze.
pub(crate) fn stop_random_ticks(world: &Path) {
    let rules = world.join("data/minecraft/game_rules.dat");
    let (path, key) = if rules.is_file() {
        (rules, None)
    } else {
        (world.join("level.dat"), Some("randomTickSpeed"))
    };
    let bytes = fs::read(&path).expect("game rules");
    let mut tag = read_compressed(&bytes, &mut NbtAccounter::unlimited_heap()).expect("game rules");
    match key {
        None => tag
            .get_compound_mut("data")
            .expect("data")
            .put_int("minecraft:random_tick_speed", 0),
        Some(key) => {
            if let Some(rules) = tag
                .get_compound_mut("Data")
                .and_then(|d| d.get_compound_mut("GameRules"))
            {
                rules.put_string(key, "0");
            }
        }
    }
    fs::write(&path, write_compressed(&tag)).expect("write game rules");
}

/// Runs `case` on every selected world; panics with the findings of every world that differs.
fn dual(case: Case) {
    let mut failures = Vec::new();
    for world in worlds() {
        let staging = tempfile::tempdir().expect("temp dir");
        let source = source_world(case, &world, staging.path());
        let ours_dir = tempfile::tempdir().expect("temp dir");
        let ours = run_on(Side::Ours, ours_dir.path(), &source, &[]);
        let mut vanilla_dirs = Vec::new();
        let (mut vanilla, mut findings);
        loop {
            vanilla_dirs.push(tempfile::tempdir().expect("temp dir"));
            vanilla = run_on(
                Side::Vanilla,
                vanilla_dirs.last().expect("dir").path(),
                &source,
                &[],
            );
            vanilla.below_zero = world.below_zero && matches!(case, Case::Normal);
            findings = compare(&vanilla, &ours);
            if findings.is_empty() || vanilla_dirs.len() > VANILLA_RETRIES {
                break;
            }
        }
        keep(case, &world, &vanilla, &ours);
        println!(
            "WORLD START {} {}: {} (vanilla run {})",
            case.name(),
            world.name,
            if findings.is_empty() {
                "equal".to_owned()
            } else {
                format!("{} finding(s)", findings.len())
            },
            vanilla_dirs.len()
        );
        if !findings.is_empty() {
            failures.push(format!(
                "== {} {}\n{}",
                case.name(),
                world.name,
                findings.join("\n")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn normal_start() {
    dual(Case::Normal);
}

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn world_newer_than_the_current_version() {
    dual(Case::Newer);
}
