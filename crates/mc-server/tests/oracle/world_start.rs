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
//!   of the loaded chunks are still exact, and so are the chunks below status full inside the rectangle.
//! * Random ticks are off in both copies of the world (`random_tick_speed` 0): they draw from an unseeded random source.
//! * Tick-driven values ([`CLOCKS`]): the two runs do not execute the same number of ticks between `Done` and the
//!   freeze. `LastPlayed` is wall-clock time. `session.lock` and `level.dat_old` are not compared (a timestamp, and the
//!   copy of the previous `level.dat`).
//! * `game_rules.dat` as a set of entries: the real server writes the rules in the order of an identity-hashed map, so even
//!   two runs of it disagree on the key order of the file ([`game_rules_difference`]).
//! * The tickets of `chunk_tickets.dat` as a set: both servers write them in the order of a hash table that they fill
//!   differently, and the key order of a ticket with a timeout differs ([`ticket_findings`]).
//! * The log: warnings, errors and the lines of [`INTEREST`] in order, the time and the thread removed, stack traces
//!   dropped, the failed request for the services' public keys left out ([`log_records`]).

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
    // compared as a set by `ticket_findings`
    "*/dimensions/*/*/data/minecraft/chunk_tickets.dat#data.tickets",
];

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
}

fn cache_dir() -> PathBuf {
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
            out.push(World { name, dir });
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
}

/// A server in `dir` (which `prepare` fills before the start) with the arguments `args`: when it starts, it freezes,
/// saves and stops; when it ends by itself (a refused world, a failed upgrade) that is the outcome.
pub(crate) fn run(
    side: Side,
    dir: &Path,
    prepare: impl FnOnce(&Path) + 'static,
    args: &[&str],
) -> Run {
    let mut builder = side.builder().dir(dir).prepare(prepare);
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
    }
}

/// A server on a copy of `world` (the directory `world` of the server directory).
fn run_on(side: Side, dir: &Path, world: &Path, args: &[&str]) -> Run {
    let world = world.to_owned();
    run(
        side,
        dir,
        move |dir| copy_dir(&world, &dir.join("world")),
        args,
    )
}

/// The records of `log` that are compared: `LEVEL message`, in order (see the module documentation).
pub(crate) fn log_records(log: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in log.lines() {
        // `[12:00:00] [Server thread/INFO]: message`; every other line is a continuation of the previous record.
        let Some((head, message)) = line.split_once("]: ") else {
            continue;
        };
        let Some((_, thread_level)) = head.rsplit_once(" [") else {
            continue;
        };
        if !line.starts_with('[') {
            continue;
        }
        let level = thread_level.rsplit('/').next().unwrap_or("");
        // a request to the Mojang services: it fails (or not) with the network, and with a different text in each server
        if message.starts_with("Failed to request Minecraft Services public key") {
            continue;
        }
        let interesting = matches!(level, "WARN" | "ERROR" | "FATAL")
            || INTEREST.iter().any(|i| message.starts_with(i));
        if interesting {
            out.push(format!("{level} {message}"));
        }
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
                .map(|k| format!("{k} = {:?}", tag.get(k)))
                .collect();
            let data = tag.get_compound_or_empty("data");
            lines.extend(data.keys().map(|k| format!("data.{k} = {:?}", data.get(k))));
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

/// The tickets of every dimension's `chunk_tickets.dat` as sorted lists of `(chunk_pos, type, level, ticks_left)`: both
/// servers hold the tickets in a fastutil table; the order they write them in and the key order of a ticket differ.
pub(crate) fn ticket_findings(vanilla: &Path, ours: &Path) -> Vec<String> {
    let tickets = |world: &Path, rel: &str| -> Vec<String> {
        let file = world.join(rel);
        if !file.is_file() {
            return Vec::new();
        }
        let tag = read_dat(&file);
        let mut list: Vec<String> = tag
            .get_compound_or_empty("data")
            .get_list("tickets")
            .map(|l| {
                l.iter()
                    .filter_map(|t| t.as_compound())
                    .map(|t| {
                        let pos = t.get_int_array("chunk_pos").unwrap_or_default();
                        format!(
                            "{pos:?} {} level {} ticks_left {:?}",
                            t.get_string_or("type", "?"),
                            t.get_int_or("level", -1),
                            t.get_long("ticks_left")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        list.sort();
        list
    };
    let mut out = Vec::new();
    for dim in ["overworld", "the_nether", "the_end"] {
        let rel = format!("dimensions/minecraft/{dim}/data/minecraft/chunk_tickets.dat");
        let (v, o) = (tickets(vanilla, &rel), tickets(ours, &rel));
        if v != o {
            out.push(format!(
                "{rel}: tickets differ\n  vanilla: {v:?}\n  ours:    {o:?}"
            ));
        }
    }
    out
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
    let semantic = Filter {
        chunks: Some(inner),
        min_status: status_rank("full").expect("status"),
        max_listed: 10,
        ignore: Vec::new(),
        exact: false,
        ignores: Ignores::default(),
    };
    let mut reports = Vec::new();
    for dim in ["overworld", "the_nether", "the_end"] {
        for rect in outside {
            reports.push(compare_dimension(&v, &o, dim, &exact(rect, &[])));
        }
        reports.push(compare_dimension(&v, &o, dim, &semantic));
        // the semantic mode skips chunks below status full: those are compared exactly, one by one
        for (x, z) in chunks_below_full(&v, &o, dim, inner) {
            reports.push(compare_dimension(&v, &o, dim, &exact((x, z, x, z), &[])));
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
    findings.extend(ticket_findings(&v, &o));
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
        let dirs = [
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        ];
        let vanilla = run_on(Side::Vanilla, dirs[0].path(), &source, &[]);
        let ours = run_on(Side::Ours, dirs[1].path(), &source, &[]);
        keep(case, &world, &vanilla, &ours);
        let findings = compare(&vanilla, &ours);
        println!(
            "WORLD START {} {}: {}",
            case.name(),
            world.name,
            if findings.is_empty() {
                "equal".to_owned()
            } else {
                format!("{} finding(s)", findings.len())
            }
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
