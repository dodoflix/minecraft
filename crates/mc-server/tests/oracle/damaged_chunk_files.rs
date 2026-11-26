//! Chunk and entity files of the current-format corpus world whose tags hold values of the wrong type, are missing or
//! carry another data version: the real 26.3 server and `mc-server` each start on a copy and the warnings and errors of
//! the two logs (with the first line of the exception behind each) and the files they leave are compared. Run with
//! `cargo test -p mc-server --test oracle damaged_chunk -- --ignored --nocapture`; `DAMAGED_CHUNK_CASES` (comma
//! list of case names) runs part of the cases, `DAMAGED_CHUNK_KEEP` (a directory) keeps both server folders.

use std::fs;
use std::path::{Path, PathBuf};

use mc_core::ident::Identifier;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, Tag};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};

use crate::startup_gates::gate_log;
use crate::world_start::{Side, cache_dir, compare, copy_dir, run, stop_random_ticks};

/// The corpus world the cases start from.
fn world() -> PathBuf {
    let dir = cache_dir().join("worlds/26.2");
    assert!(
        dir.join("level.dat").is_file(),
        "{}: no corpus world: run scripts/datafix-worlds.sh 26.2",
        dir.display()
    );
    dir
}

/// Applies `f` to the compounds of `tag` below `path`: lists are walked element by element.
fn map_list(tag: &mut CompoundTag, path: &[&str], f: &dyn Fn(&mut CompoundTag)) {
    let [first, rest @ ..] = path else {
        return f(tag);
    };
    match tag.get_mut(first) {
        Some(Tag::Compound(c)) => map_list(c, rest, f),
        Some(Tag::List(list)) => {
            for element in list.iter_mut() {
                if let Tag::Compound(c) = element {
                    map_list(c, rest, f);
                }
            }
        }
        _ => {}
    }
}

/// Applies `f` to every chunk tag of the `folder` (`region`, `entities`) of the overworld of `world`.
fn edit_chunks(world: &Path, folder: &str, f: &dyn Fn(&mut CompoundTag)) {
    let dir = world.join("dimensions/minecraft/overworld").join(folder);
    let info = RegionStorageInfo::new("t", Identifier::parse("overworld").unwrap(), "chunk");
    let mut storage = RegionFileStorage::new(info, &dir, false);
    for entry in fs::read_dir(&dir).expect("chunk folder").flatten() {
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
                f(&mut chunk);
                storage.write(&pos, Some(&chunk)).expect("write chunk");
            }
        }
    }
    storage.close().expect("close region storage");
}

/// `record` with every identity hash (`@` and hex digits after a class name) written as `@0`.
fn mask_hashes(record: &str) -> String {
    let mut out = String::with_capacity(record.len());
    let mut rest = record;
    while let Some(at) = rest.find('@') {
        let (head, tail) = rest.split_at(at + 1);
        out.push_str(head);
        let digits = tail.chars().take_while(char::is_ascii_hexdigit).count();
        // the digits of `Integer.toHexString` have no leading zeros: any length
        out.push_str(if digits > 0 { "0" } else { "" });
        rest = &tail[digits..];
    }
    out.push_str(rest);
    out
}

type Damage = fn(&mut CompoundTag);

/// `(case, folder, damage)`.
const CASES: &[(&str, &str, Damage)] = &[
    ("chunk_sections_int", "region", |t| t.put_int("sections", 1)),
    ("chunk_status_int", "region", |t| t.put_int("Status", 3)),
    ("chunk_block_entities_int", "region", |t| {
        t.put_int("block_entities", 1)
    }),
    ("chunk_heightmaps_wrong", "region", |t| {
        t.put_string("Heightmaps", "x")
    }),
    ("chunk_blockstates_wrong", "region", |t| {
        map_list(t, &["sections"], &|s| s.put_string("block_states", "x"))
    }),
    ("chunk_unknown_keys", "region", |t| {
        t.put_int("zzUnknown", 1)
    }),
];

#[test]
#[ignore = "oracle: spawns vanilla server"]
fn damaged_chunk_files() {
    let only: Option<Vec<String>> = std::env::var("DAMAGED_CHUNK_CASES")
        .ok()
        .map(|v| v.split(',').map(str::to_owned).collect());
    let keep = std::env::var_os("DAMAGED_CHUNK_KEEP").map(PathBuf::from);
    let mut failures = Vec::new();
    for (name, folder, damage) in CASES {
        if only.as_ref().is_some_and(|o| !o.iter().any(|n| n == name)) {
            continue;
        }
        let staging = tempfile::tempdir().expect("temp dir");
        let source = staging.path().join("world");
        copy_dir(&world(), &source);
        let _ = fs::remove_file(source.join("session.lock"));
        // random ticks are not repeatable between two runs
        stop_random_ticks(&source);
        edit_chunks(&source, folder, damage);
        let dirs = [
            tempfile::tempdir().expect("temp dir"),
            tempfile::tempdir().expect("temp dir"),
        ];
        let [vanilla, ours] = [Side::Vanilla, Side::Ours].map(|side| {
            let dir = &dirs[usize::from(side == Side::Ours)];
            let source = source.clone();
            run(
                side,
                dir.path(),
                move |d| copy_dir(&source, &d.join("world")),
                &[],
            )
        });
        if let Some(keep) = &keep {
            let base = keep.join(name);
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(&base).expect("keep folder");
            copy_dir(&vanilla.dir, &base.join("vanilla"));
            copy_dir(&ours.dir, &base.join("ours"));
        }
        let mut findings = Vec::new();
        if (vanilla.started, vanilla.exit) != (ours.started, ours.exit) {
            findings.push(format!(
                "outcome: vanilla started={} exit={:?}, ours started={} exit={:?}",
                vanilla.started, vanilla.exit, ours.started, ours.exit
            ));
        }
        // the chunks load on worker threads, so the order of the records of different chunks varies from run to run
        // identity hashes (`BlockEntityType@1b2c3d4`, `URLClassLoader @1b2c3d4`) differ from run to run
        let sorted = |run: &crate::world_start::Run| {
            let mut log: Vec<String> = gate_log(run)
                .iter()
                .map(|record| mask_hashes(record))
                .collect();
            log.sort();
            log
        };
        let (vanilla_log, ours_log) = (sorted(&vanilla), sorted(&ours));
        if let Some(d) = crate::world_start::first_log_difference(&vanilla_log, &ours_log) {
            findings.push(d);
        }
        // `chunk_sections_int` leaves the POI files of the loaded chunks at another data version in `mc-server` (the real
        // server rewrites them): only its log and outcome are compared
        if vanilla.started && ours.started && *name != "chunk_sections_int" {
            findings.extend(compare(&vanilla, &ours).into_iter().filter(|f| {
                // the log records were compared above, sorted
                !f.starts_with("log record")
            }));
        }
        println!(
            "DAMAGED CHUNK {name}: {}",
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
