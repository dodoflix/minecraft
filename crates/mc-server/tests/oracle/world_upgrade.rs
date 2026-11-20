//! `--forceUpgrade`, `--eraseCache` and `--recreateRegionFiles`: the same world is upgraded by the real vanilla server and
//! by `mc-server`, and the two world directories are compared file by file (`worldcmp --tree`, chunk NBT exact) together
//! with the lines the upgrade logs.
//!
//! The worlds come from `scripts/datafix-worlds.sh` (`$MC_DATAFIX_CACHE`, default `~/.cache/mc-datafix-corpus`). A server
//! starts after the upgrade and keeps running until the test stops it, so every chunk it loads is saved again with what
//! the run changed. Most cases give the servers the world with its forced chunks removed and its spawn moved to a far
//! corner ([`prepare_world`]): what they load and generate there is left out of the comparison, and every stored chunk
//! is compared exactly (tags, key order, bytes) as the upgrade left it, except for the two things vanilla itself does
//! not repeat between runs ([`mend_lost_last_writes`], the `sherds` ignore). The `loaded_chunks_*` cases keep spawn and
//! forced chunks as stored and say what they leave out ([`loaded`]). The `*_stream_*` cases damage the compressed
//! stream of one chunk and compare what the servers log. `WORLD_UPGRADE_KEEP=<dir>` keeps both server directories of
//! a case under `<dir>/upgrade/<case>/{vanilla,ours}`. Run with
//! `MC_ACCEPT_EULA=1 cargo test -p mc-server --test oracle world_upgrade -- --ignored`.
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::common::world_diff::{list_files, read_region};
use crate::world_start::ticket_findings;
use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_testkit::Server;
use mc_world::level_storage::WORLD_VERSION;
use mc_world::region::{RegionFile, RegionFileStorage, RegionStorageInfo};
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

/// A copy of the corpus world for a server: as it is (`moved` false), or without forced chunks and with the spawn at
/// [`FAR_SPAWN`].
fn prepare_world(from: &Path, to: &Path, moved: bool) {
    copy_dir(from, to);
    if !moved {
        return;
    }
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

/// What the upgrade logged: `Forcing world upgrade!`, every line of the `World Upgrader` thread (the seconds of the last
/// one masked) and the warnings about chunks that could not be read.
fn upgrade_log(server: &Server) -> Vec<String> {
    server
        .log()
        .lines()
        .filter_map(message)
        .filter(|m| {
            m.starts_with("World Upgrader #")
                || m.contains("Forcing world upgrade!")
                || m.contains("/WARN: Failed to read chunk")
        })
        // vanilla reads on an `IO-Worker-<n>` thread, this server on the upgrader's own
        .map(|m| match m.split_once("/WARN: Failed to read chunk") {
            Some((_, rest)) => format!("IO-Worker/WARN: Failed to read chunk{rest}"),
            None => m,
        })
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
/// one is replaced by ours, byte for byte; a second one in the same folder is a real difference and stays.
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
                .is_some_and(|c| data_version(c) == WORLD_VERSION);
            if !upgraded || data_version(chunk) == WORLD_VERSION {
                continue;
            }
            if !lost_in_folder.insert(folder.to_path_buf()) {
                continue;
            }
            let info = || {
                RegionStorageInfo::new(
                    "world",
                    Identifier::parse("minecraft:overworld").unwrap(),
                    "chunk",
                )
            };
            // the stored bytes as they are: reading the tag and writing it again would order colliding keys by the order
            // they were read in
            let at = ChunkPos::new(pos.0, pos.1);
            let mut raw = Vec::new();
            let source = RegionFile::new(info(), &mine, mine.parent().expect("folder"), false)
                .expect("open our region file");
            source
                .get_chunk_data_input_stream(&at)
                .expect("read the chunk")
                .expect("the chunk")
                .read_to_end(&mut raw)
                .expect("decompress the chunk");
            source.close().expect("close our region file");
            let mut target = RegionFile::new(info(), &theirs, folder, false)
                .expect("open the vanilla region file");
            target
                .write_chunk_data(&at, &raw)
                .expect("write the chunk vanilla lost");
            target.close().expect("close the vanilla region file");
            println!("vanilla lost the write of chunk {pos:?} in {rel}");
        }
    }
}

struct Outcome {
    log: Vec<String>,
    dir: PathBuf,
}

/// One server (`ours` or vanilla) started in `dir` over a copy of the world `id` (or `world`) with the upgrade `flags`,
/// stopped again; `moved` as for [`prepare_world`].
fn upgrade(
    ours: bool,
    id: &str,
    flags: &[&str],
    dir: &Path,
    world: Option<&Path>,
    moved: bool,
) -> Outcome {
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
        .prepare(move |dir| prepare_world(&source, &dir.join("world"), moved))
        .start_unready()
        .unwrap_or_else(|e| panic!("start {}: {e}", if ours { "ours" } else { "vanilla" }));
    // vanilla builds a crash report when an upgrade fails, which takes minutes on a busy machine
    assert!(
        server.wait_log(r"Done \(", 900).is_some(),
        "the server did not finish starting:\n{}",
        server.log()
    );
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

/// `WORLD_UPGRADE_KEEP=<dir>`: both server directories of a case are copied to `<dir>/upgrade/<case>/{vanilla,ours}`.
fn keep(case: &str, vanilla: &Outcome, ours: &Outcome) {
    let Some(root) = std::env::var_os("WORLD_UPGRADE_KEEP") else {
        return;
    };
    let base = PathBuf::from(root).join("upgrade").join(case);
    let _ = std::fs::remove_dir_all(&base);
    for (name, outcome) in [("vanilla", vanilla), ("ours", ours)] {
        copy_dir(
            outcome.dir.parent().expect("server directory"),
            &base.join(name),
        );
    }
}

/// Both servers upgrade the world; their logs and world directories must be equal.
fn both(case: &str, id: &str, flags: &[&str]) {
    let vanilla = upgrade(false, id, flags, &scratch(case).join("vanilla"), None, true);
    let ours = upgrade(true, id, flags, &scratch(case).join("ours"), None, true);
    keep(case, &vanilla, &ours);
    compare(case, flags, &vanilla, &ours, false);
}

fn compare(case: &str, flags: &[&str], vanilla: &Outcome, ours: &Outcome, loaded: bool) {
    assert!(
        vanilla.log.iter().any(|l| l.contains("Upgrading blocks")),
        "{case}: vanilla did not upgrade anything:\n{}",
        vanilla.log.join("\n")
    );
    assert_eq!(vanilla.log, ours.log, "{case}: the upgrade log");
    let ignores = Ignores(
        run_dependent()
            .iter()
            .map(String::as_str)
            .chain(LOADED_IGNORES.iter().copied().filter(|_| loaded))
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
    if loaded {
        drop_light_only_sections(&vanilla.dir);
        drop_light_only_sections(&ours.dir);
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

/// What differs between two runs of vanilla itself in the chunks the server loads and saves (three runs of the flat world
/// differ in each of these): the chunks' `LastUpdate` (the game time of the last save: the number of ticks that ran before
/// the stop), the order of the `attributes` of the saved entities (a hash map keyed by the identity hash of the attribute
/// holders) and the order of the tickets in `chunk_tickets.dat`, which follows the tickets the loading added and removed
/// on the way (the tickets are compared as a set, [`ticket_findings`]).
const LOADED_IGNORES: &[&str] = &[
    "**.mca#LastUpdate",
    "**.mca#Entities.*.attributes",
    "dimensions/*/*/data/minecraft/chunk_tickets.dat#data.tickets",
];

/// A loaded chunk keeps a section that holds only light when the light engine had data for it at the save, and that
/// depends on whether its updates ran before the stop: the same run of vanilla has it in one chunk and the next run in
/// another. Every chunk of the `region` files of both worlds is written again without them. (That also writes the keys
/// of colliding hash buckets in the order they were read in, the same on both sides.)
fn drop_light_only_sections(world: &Path) {
    for rel in list_files(world).iter().filter(|f| f.ends_with(".mca")) {
        let path = world.join(rel);
        let folder = path.parent().expect("folder");
        if folder.file_name().is_none_or(|n| n != "region") {
            continue;
        }
        let mut storage = RegionFileStorage::new(
            RegionStorageInfo::new(
                "world",
                Identifier::parse("minecraft:overworld").unwrap(),
                "chunk",
            ),
            folder,
            false,
        );
        for ((x, z), mut chunk) in read_region(&path) {
            if let Some(sections) = chunk.get_list_mut("sections") {
                let light_only: Vec<usize> = (0..sections.len())
                    .filter(|&i| {
                        sections
                            .get_compound(i)
                            .is_some_and(|s| !s.contains("block_states"))
                    })
                    .collect();
                for i in light_only.into_iter().rev() {
                    sections.remove(i);
                }
            }
            storage
                .write(&ChunkPos::new(x, z), Some(&chunk))
                .expect("write the chunk without its light-only sections");
        }
        storage.close().expect("close the storage");
    }
}

/// Both servers upgrade the world with its spawn and forced chunks as stored, so they load, tick a little and save the
/// chunks around (0, 0) when they stop. What is left out of the comparison differs between two runs of vanilla itself
/// ([`LOADED_IGNORES`], [`drop_light_only_sections`]).
fn loaded(case: &str, id: &str) {
    let flags = ["--forceUpgrade"];
    let vanilla = upgrade(
        false,
        id,
        &flags,
        &scratch(case).join("vanilla"),
        None,
        false,
    );
    let ours = upgrade(true, id, &flags, &scratch(case).join("ours"), None, false);
    keep(case, &vanilla, &ours);
    let tickets = ticket_findings(&vanilla.dir, &ours.dir);
    assert!(tickets.is_empty(), "{case}: {}", tickets.join("\n"));
    compare(case, &flags, &vanilla, &ours, true);
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn loaded_chunks_flat() {
    loaded("loaded-flat", "26.2");
}

#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn loaded_chunks_normal() {
    loaded("loaded-normal", "26.2-normal");
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
        true,
    );
    let vanilla = upgrade(
        false,
        "26.2-normal",
        &["--forceUpgrade"],
        &scratch(case).join("vanilla"),
        Some(&first.dir),
        true,
    );
    let ours = upgrade(
        true,
        "26.2-normal",
        &["--forceUpgrade"],
        &scratch(case).join("ours"),
        Some(&first.dir),
        true,
    );
    keep(case, &vanilla, &ours);
    compare(case, &["--forceUpgrade"], &vanilla, &ours, false);
}

/// Where the stream of the first big deflate chunk of `region` (a region file's bytes) lies: `(start, length)`.
fn big_stream(region: &[u8]) -> (usize, usize) {
    let word = |at: usize| u32::from_be_bytes(region[at..at + 4].try_into().expect("4 bytes"));
    (0..1024)
        .filter_map(|i| {
            let start = (word(i * 4) >> 8) as usize * 4096;
            (start != 0 && region[start + 4] == 2 && word(start) > 6000)
                .then(|| (start + 5, word(start) as usize - 1))
        })
        .next()
        .expect("a big deflate chunk")
}

/// The offset in `stream` of the first byte that, inverted, makes the inflater fail: before its first bytes come out
/// (`early`), or after them.
fn first_breaking_byte(stream: &[u8], early: bool, from: usize) -> usize {
    use std::io::Read;
    (from..stream.len())
        .find(|&at| {
            let mut broken = stream.to_vec();
            broken[at] ^= 0xff;
            let mut reader = mc_world::region::region_file_version::VERSION_DEFLATE
                .wrap_input(broken)
                .expect("reader");
            let first = reader.read(&mut [0u8; 8192]);
            first.is_err() == early
                && (early || std::io::copy(&mut reader, &mut std::io::sink()).is_err())
        })
        .expect("a byte that breaks the stream")
}

/// A copy of the corpus world in which `damage` changed the big chunk of the overworld's first region file: it gets
/// the stream's offset in the file and the stream itself, and may change the chunk's declared length (at `start - 5`).
fn damaged_world(case: &str, damage: impl FnOnce(&mut [u8], usize, usize)) -> PathBuf {
    let dest = scratch(case).join("source");
    let _ = std::fs::remove_dir_all(&dest);
    copy_dir(&corpus_world("26.2-normal"), &dest);
    let file = dest.join("dimensions/minecraft/overworld/region/r.0.0.mca");
    let mut bytes = std::fs::read(&file).expect("region file");
    let (start, length) = big_stream(&bytes);
    damage(&mut bytes, start, length);
    std::fs::write(&file, bytes).expect("write the region file");
    dest
}

/// Both servers upgrade a world with one damaged chunk stream; the logs must be equal, and vanilla itself must have
/// skipped the chunk (`aborts` false: `Error upgrading chunk`) or ended the upgrade (`Error upgrading world`). The
/// stored chunks are not compared: after an abort they depend on which dimension the upgrader reached first, which
/// differs between two runs of vanilla.
fn damaged_stream(case: &str, aborts: bool, source: &Path) {
    let flags = ["--forceUpgrade"];
    let vanilla = upgrade(
        false,
        "26.2-normal",
        &flags,
        &scratch(case).join("vanilla"),
        Some(source),
        true,
    );
    let ours = upgrade(
        true,
        "26.2-normal",
        &flags,
        &scratch(case).join("ours"),
        Some(source),
        true,
    );
    keep(case, &vanilla, &ours);
    let log = vanilla.log.join("\n");
    assert!(
        log.contains("Error upgrading world") == aborts
            && log.contains("Error upgrading chunk") != aborts,
        "{case}: vanilla's upgrade log:\n{log}"
    );
    assert_eq!(vanilla.log, ours.log, "{case}: the upgrade log");
}

/// The zlib header of a chunk is wrong: the first refill of the inflater fails, vanilla skips the chunk.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn corrupt_stream_header() {
    let source = damaged_world("corrupt-header", |b, start, _| b[start..start + 2].fill(0));
    damaged_stream("corrupt-header", false, &source);
}

/// A byte in the first 512 compressed bytes breaks the stream: the first inflate call fails, vanilla skips the chunk.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn corrupt_stream_early() {
    let source = damaged_world("corrupt-early", |b, start, length| {
        b[start + first_breaking_byte(&b[start..start + length], true, 2)] ^= 0xff;
    });
    damaged_stream("corrupt-early", false, &source);
}

/// A byte after the first inflate calls breaks the stream: the tag loader wraps the failure, vanilla ends the upgrade.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn corrupt_stream_late() {
    let source = damaged_world("corrupt-late", |b, start, length| {
        b[start + first_breaking_byte(&b[start..start + length], false, 2000)] ^= 0xff;
    });
    damaged_stream("corrupt-late", true, &source);
}

/// The stream ends after 3000 bytes: the inflater reaches the end of its input in a later refill, vanilla ends the upgrade.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn truncated_stream_late() {
    let source = damaged_world("truncated", |b, start, _| {
        b[start - 5..start - 1].copy_from_slice(&3001u32.to_be_bytes());
    });
    damaged_stream("truncated", true, &source);
}

/// The stream ends right after the zlib header: the first refill reaches the end of its input, vanilla skips the chunk.
#[test]
#[ignore = "oracle: starts the vanilla server and the mc-server binary"]
fn truncated_stream_early() {
    let source = damaged_world("truncated-early", |b, start, _| {
        b[start - 5..start - 1].copy_from_slice(&3u32.to_be_bytes());
    });
    damaged_stream("truncated-early", false, &source);
}
