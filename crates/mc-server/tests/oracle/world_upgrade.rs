//! `--forceUpgrade`, `--eraseCache` and `--recreateRegionFiles`: the same world is upgraded by the real vanilla server and
//! by `mc-server`, and the two world directories are compared file by file (`worldcmp --tree`, chunk NBT exact) together
//! with the lines the upgrade logs.
//!
//! The worlds come from `scripts/datafix-worlds.sh` (`$MC_DATAFIX_CACHE`, default `~/.cache/mc-datafix-corpus`). A server
//! starts after the upgrade and keeps running until the test stops it, so every chunk it loads is saved again with what
//! the run changed. The servers get the world with its forced chunks removed and its spawn moved to a far corner
//! ([`prepare_world`]): what they load and generate there is left out of the comparison, and every stored chunk is
//! compared exactly (tags, key order, bytes) as the upgrade left it, except for the two things vanilla itself does not
//! repeat between runs ([`mend_lost_last_writes`], the `sherds` ignore). Run with
//! `MC_ACCEPT_EULA=1 cargo test -p mc-server --test oracle world_upgrade -- --ignored`.
use std::path::{Path, PathBuf};

use crate::common::world_diff::{list_files, read_region};
use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_testkit::Server;
use mc_world::level_storage::WORLD_VERSION;
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use worldcmp::fields::{FieldSpec, Ignores};
use worldcmp::tree::{TreeOptions, compare_trees, render_text};

/// `$MC_DATAFIX_CACHE/worlds/<id>`: a world an old vanilla server saved.
fn corpus_world(id: &str) -> PathBuf {
    let cache = std::env::var_os("MC_DATAFIX_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".cache")
                })
                .join("mc-datafix-corpus")
        });
    let world = cache.join("worlds").join(id);
    assert!(
        world.join("level.dat").is_file(),
        "{}: no such world, run scripts/datafix-worlds.sh {id}",
        world.display()
    );
    world
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the copy");
    for entry in std::fs::read_dir(from).expect("read the world") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

/// Block coordinate of the moved spawn: chunk 125 000 of region 3906, far from every stored chunk.
const FAR_SPAWN: i32 = 2_000_000;
/// The region file the servers generate around the moved spawn.
const FAR_REGION: &str = "r.3906.3906.mca";

fn remove_forced_chunks(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("read the world").flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_forced_chunks(&path);
        } else if matches!(
            entry.file_name().to_str(),
            Some("chunk_tickets.dat" | "chunks.dat")
        ) {
            std::fs::remove_file(&path).expect("remove the forced chunks");
        }
    }
}

/// A copy of the corpus world for a server: without forced chunks and with the spawn at [`FAR_SPAWN`].
fn prepare_world(from: &Path, to: &Path) {
    copy_dir(from, to);
    remove_forced_chunks(to);
    let path = to.join("level.dat");
    let bytes = std::fs::read(&path).expect("read level.dat");
    let mut level =
        mc_nbt::compress::read_compressed(&bytes, &mut mc_nbt::NbtAccounter::unlimited_heap())
            .expect("decode level.dat");
    let data = level.get_compound_mut("Data").expect("Data");
    if let Some(spawn) = data.get_compound_mut("spawn") {
        spawn.put_int_array("pos", vec![FAR_SPAWN, 80, FAR_SPAWN]);
    } else {
        data.put_int("SpawnX", FAR_SPAWN);
        data.put_int("SpawnZ", FAR_SPAWN);
    }
    std::fs::write(&path, mc_nbt::compress::write_compressed(&level)).expect("write level.dat");
}

/// `[12:00:00] [Thread/INFO]: text` as `Thread/INFO: text`.
fn message(line: &str) -> Option<String> {
    let rest = line.strip_prefix('[')?.split_once("] ")?.1;
    let rest = rest.strip_prefix('[')?;
    let (thread_level, text) = rest.split_once("]: ")?;
    Some(format!("{thread_level}: {text}"))
}

/// What the upgrade logged: `Forcing world upgrade!` and every line of the `World Upgrader` thread, the seconds of the
/// last one masked.
fn upgrade_log(server: &Server) -> Vec<String> {
    server
        .log()
        .lines()
        .filter_map(message)
        .filter(|m| m.starts_with("World Upgrader #") || m.contains("Forcing world upgrade!"))
        .map(|m| match m.split_once(" finished after ") {
            Some((head, _)) => format!("{head} finished after <n> seconds"),
            None => m,
        })
        .map(|m| m.replacen("ServerMain/", "main/", 1))
        .collect()
}

/// What differs between two runs of the same server: the clock and the counters of the ticks that ran, from the
/// specs of the world generator script; the dimension, entity and chunk field lines of that list do not apply, the
/// stored chunks are compared whole.
fn run_dependent() -> Vec<String> {
    let mut specs: Vec<String> =
        include_str!("../../../mc-datafix/tests/fixtures/world_run_dependent.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty() && !l.starts_with("**.mca#"))
            .filter(|l| !l.ends_with('/'))
            .map(str::to_owned)
            .collect();
    // what the server generates around the moved spawn
    for folder in ["region", "entities", "poi"] {
        specs.push(format!(
            "dimensions/minecraft/overworld/{folder}/{FAR_REGION}"
        ));
        specs.push(format!("{folder}/{FAR_REGION}"));
    }
    // the tick counters of the saved data of the dimensions
    specs.push("dimensions/*/*/data/minecraft/raids.dat#data.tick".to_owned());
    // the side order of a pot's `sherds` is the iteration order of a `HashMap` keyed by `Dynamic`s, whose hash includes the
    // identity hash of `NbtOps.INSTANCE`: it differs between two runs of vanilla itself
    specs.push("**.mca#block_entities.*.sherds".to_owned());
    specs
}

/// The data version a chunk is stored with (`-1` without one).
fn data_version(tag: &mc_nbt::CompoundTag) -> i32 {
    tag.get_int_or("DataVersion", -1)
}

/// Vanilla's `IOWorker` drops a write whose task starts after `close()` set `shutdownRequested`, and nothing waits for the
/// last write of a folder before the upgrader closes its storage, so vanilla sometimes leaves the last chunk it
/// upgraded in a folder as it was (`--recreateRegionFiles` joins it first). Which chunk is lost depends on timing; ours
/// always writes it. For each folder, a chunk that vanilla still has in an older version where ours has the current
/// one is replaced by ours; a second one in the same folder is a real difference and stays.
fn mend_lost_last_writes(vanilla: &Path, ours: &Path) {
    let mut lost_in_folder = std::collections::BTreeSet::new();
    for rel in list_files(vanilla).iter().filter(|f| f.ends_with(".mca")) {
        let (theirs, mine) = (vanilla.join(rel), ours.join(rel));
        if !mine.is_file() {
            continue;
        }
        let (theirs_chunks, my_chunks) = (read_region(&theirs), read_region(&mine));
        let folder = theirs.parent().expect("folder");
        for (pos, chunk) in &theirs_chunks {
            let upgraded = my_chunks
                .get(pos)
                .filter(|c| data_version(c) == WORLD_VERSION);
            let (Some(upgraded), true) = (upgraded, data_version(chunk) != WORLD_VERSION) else {
                continue;
            };
            if !lost_in_folder.insert(folder.to_path_buf()) {
                continue;
            }
            let info = RegionStorageInfo::new(
                "world",
                Identifier::parse("minecraft:overworld").unwrap(),
                "chunk",
            );
            let mut storage = RegionFileStorage::new(info, folder, false);
            storage
                .write(&ChunkPos::new(pos.0, pos.1), Some(upgraded))
                .expect("write the chunk vanilla lost");
            storage.close().expect("close the storage");
            println!("vanilla lost the write of chunk {pos:?} in {rel}");
        }
    }
}

struct Outcome {
    log: Vec<String>,
    dir: PathBuf,
}

/// One server (`ours` or vanilla) started in `dir` over a copy of the world `id` with the upgrade `flags`, stopped again.
fn upgrade(ours: bool, id: &str, flags: &[&str], dir: &Path, world: Option<&Path>) -> Outcome {
    let _ = std::fs::remove_dir_all(dir);
    let source = world.map_or_else(|| corpus_world(id), Path::to_path_buf);
    let mut builder = if ours {
        super::common::ours(1, "survival", &[])
    } else {
        mc_testbot::harness::vanilla(1, "survival", &[])
    };
    for flag in flags {
        builder = builder.arg(flag);
    }
    let server = builder
        .dir(dir)
        .prepare(move |dir| prepare_world(&source, &dir.join("world")))
        .start()
        .unwrap_or_else(|e| panic!("start {}: {e}", if ours { "ours" } else { "vanilla" }));
    let log = upgrade_log(&server);
    assert!(server.stop(), "the server did not stop cleanly");
    Outcome {
        log,
        dir: dir.join("world"),
    }
}

fn scratch(case: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("world-upgrade")
        .join(case)
}

/// Both servers upgrade the world; their logs and world directories must be equal.
fn both(case: &str, id: &str, flags: &[&str]) -> (Outcome, Outcome) {
    let vanilla = upgrade(false, id, flags, &scratch(case).join("vanilla"), None);
    let ours = upgrade(true, id, flags, &scratch(case).join("ours"), None);
    compare(case, flags, &vanilla, &ours);
    (vanilla, ours)
}

fn compare(case: &str, flags: &[&str], vanilla: &Outcome, ours: &Outcome) {
    assert!(
        vanilla.log.iter().any(|l| l.contains("Upgrading blocks")),
        "{case}: vanilla did not upgrade anything:\n{}",
        vanilla.log.join("\n")
    );
    assert_eq!(vanilla.log, ours.log, "{case}: the upgrade log");
    let ignores = Ignores(
        run_dependent()
            .iter()
            .map(|s| s.parse::<FieldSpec>().expect("ignore spec"))
            .collect(),
    );
    let options = TreeOptions {
        ignores,
        chunks: None,
        max_listed: 50,
    };
    // `--recreateRegionFiles` waits for the last write of every file: no write can be lost
    if !flags.contains(&"--recreateRegionFiles") {
        mend_lost_last_writes(&vanilla.dir, &ours.dir);
    }
    let report = compare_trees(&vanilla.dir, &ours.dir, &options);
    assert!(
        report.is_clean(),
        "{case}:\n{}",
        render_text("vanilla", "ours", &report, 50)
    );
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn force_upgrade_flat() {
    both("flat", "26.2", &["--forceUpgrade"]);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn force_upgrade_normal() {
    both("normal", "26.2-normal", &["--forceUpgrade"]);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn erase_cache() {
    both("erase", "26.2-normal", &["--forceUpgrade", "--eraseCache"]);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn recreate_region_files() {
    both("recreate", "26.2-normal", &["--recreateRegionFiles"]);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn all_flags() {
    both(
        "all",
        "26.2-normal",
        &["--forceUpgrade", "--eraseCache", "--recreateRegionFiles"],
    );
}

/// A second upgrade of an upgraded world skips its chunks: both servers start from the vanilla result.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn second_upgrade_skips() {
    let case = "again";
    let first = upgrade(
        false,
        "26.2-normal",
        &["--forceUpgrade"],
        &scratch(case).join("first"),
        None,
    );
    let vanilla = upgrade(
        false,
        "26.2-normal",
        &["--forceUpgrade"],
        &scratch(case).join("vanilla"),
        Some(&first.dir),
    );
    let ours = upgrade(
        true,
        "26.2-normal",
        &["--forceUpgrade"],
        &scratch(case).join("ours"),
        Some(&first.dir),
    );
    compare(case, &["--forceUpgrade"], &vanilla, &ours);
}
