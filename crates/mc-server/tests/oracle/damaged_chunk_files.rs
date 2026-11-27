//! Chunk and entity files of the current-format corpus world (the `old_region` cases: the 1.16.5 world) whose tags hold
//! values of the wrong type, are missing or carry another data version: the real 26.3 server and `mc-server` each start
//! on a copy and the warnings and errors of the two logs (with the first line of the exception behind each) and the
//! files they leave are compared. Run with `cargo test -p mc-server --test oracle damaged_chunk -- --ignored
//! --nocapture`; `DAMAGED_CHUNK_CASES` (comma list of case names) runs part of the cases, `DAMAGED_CHUNK_KEEP` (a
//! directory) keeps both server folders.

use std::fs;
use std::path::{Path, PathBuf};

use mc_core::ident::Identifier;
use mc_core::java_string::JavaString;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, ListTag, Tag};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};

use crate::startup_gates::gate_log;
use crate::world_start::{Side, cache_dir, compare, copy_dir, run, stop_random_ticks};

/// The corpus world the cases of `folder` start from: the 1.16.5 world for `old_region`, else the 26.2 world.
fn world(folder: &str) -> PathBuf {
    let dir = cache_dir().join(if folder == "old_region" {
        "worlds/1.16.5"
    } else {
        "worlds/26.2"
    });
    assert!(
        dir.join("level.dat").is_file(),
        "{}: no corpus world: run scripts/datafix-worlds.sh",
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
    let dir = if folder == "old_region" {
        world.join("region")
    } else {
        world.join("dimensions/minecraft/overworld").join(folder)
    };
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

/// The chunks of an `entities` folder that hold entities, in position order.
fn entity_chunks(dir: &Path) -> Vec<(i32, i32)> {
    let info = RegionStorageInfo::new("t", Identifier::parse("overworld").unwrap(), "entities");
    let mut storage = RegionFileStorage::new(info, dir, false);
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut parts = name.strip_suffix(".mca").unwrap_or("").split('.').skip(1);
        let (Some(Ok(rx)), Some(Ok(rz))) = (
            parts.next().map(str::parse::<i32>),
            parts.next().map(str::parse::<i32>),
        ) else {
            continue;
        };
        for pos in (0..1024).map(|i| ChunkPos::new(rx * 32 + i % 32, rz * 32 + i / 32)) {
            let chunk = storage.read(&pos).expect("read chunk");
            if chunk.is_some_and(|c| c.get_list("Entities").is_some_and(|l| !l.is_empty())) {
                out.push((pos.x, pos.z));
            }
        }
    }
    out.sort_unstable();
    out
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

/// Gives every block entity of a chunk the type `id`.
fn set_block_entity_ids(chunk: &mut CompoundTag, id: &str) {
    map_list(chunk, &["block_entities"], &|e| e.put_string("id", id));
}

/// A string holding a lone high surrogate: vanilla's log file writes it as `?`.
fn lone_surrogate() -> JavaString {
    JavaString::from_utf16_units(&[0xD800])
}

type Damage = fn(&mut CompoundTag);

/// `(case, folder, damage)`.
const CASES: &[(&str, &str, Damage)] = &[
    ("chunk_sections_int", "region", |t| t.put_int("sections", 1)),
    ("chunk_status_int", "region", |t| t.put_int("Status", 3)),
    ("chunk_status_unknown", "region", |t| {
        t.put_string("Status", "minecraft:zzz")
    }),
    ("chunk_status_surrogate", "region", |t| {
        t.put_string("Status", lone_surrogate())
    }),
    ("chunk_xpos_mismatch", "region", |t| t.put_int("xPos", 99)),
    ("chunk_data_version_string", "region", |t| {
        t.put_string("DataVersion", "x")
    }),
    ("chunk_no_data_version", "region", |t| {
        t.remove("DataVersion");
    }),
    ("chunk_old_data_version", "region", |t| {
        t.put_int("DataVersion", 2000)
    }),
    ("chunk_block_entities_int", "region", |t| {
        t.put_int("block_entities", 1)
    }),
    ("chunk_heightmaps_wrong", "region", |t| {
        t.put_string("Heightmaps", "x")
    }),
    ("chunk_blockstates_wrong", "region", |t| {
        map_list(t, &["sections"], &|s| s.put_string("block_states", "x"))
    }),
    ("chunk_blockstates_surrogate", "region", |t| {
        map_list(t, &["sections"], &|s| {
            let mut palette = ListTag::new();
            palette.push(Tag::String(lone_surrogate()));
            let mut states = CompoundTag::new();
            states.put("palette", palette);
            s.put("block_states", states);
        })
    }),
    // every block entity of the world becomes a type that does not fit its block (sign, hanging sign and skull accept
    // more than one block, the banner constructor casts the block, a chest only accepts chests)
    ("chunk_block_entity_id_banner", "region", |t| {
        set_block_entity_ids(t, "minecraft:banner")
    }),
    ("chunk_block_entity_id_chest", "region", |t| {
        set_block_entity_ids(t, "minecraft:chest")
    }),
    ("chunk_block_entity_id_hanging_sign", "region", |t| {
        set_block_entity_ids(t, "minecraft:hanging_sign")
    }),
    ("chunk_block_entity_id_skull", "region", |t| {
        set_block_entity_ids(t, "minecraft:skull")
    }),
    ("chunk_unknown_keys", "region", |t| {
        t.put_int("zzUnknown", 1)
    }),
    // a stored chunk 13 chunks from the forced ones is read from disk far below `FULL`: its legacy entities wait for the
    // `FULL` step (a world of the 1.16.5 layout, where every chunk is stored `full`)
    ("old_chunk_legacy_entities_far", "old_region", |t| {
        if let Some(level) = t.get_compound_mut("Level")
            && (level.get_int_or("xPos", 0), level.get_int_or("zPos", 0)) == (-13, 9)
        {
            let mut pig = CompoundTag::new();
            pig.put_string("id", "minecraft:pig");
            let mut pos = ListTag::new();
            for v in [-207.5, -60.0, 152.5] {
                pos.push(Tag::double(v));
            }
            pig.put("Pos", pos);
            let mut entities = ListTag::new();
            entities.push(Tag::Compound(pig));
            level.put("Entities", entities);
        }
    }),
    ("ent_data_version_string", "entities", |t| {
        t.put_string("DataVersion", "x")
    }),
    ("ent_entities_int", "entities", |t| t.put_int("Entities", 1)),
    ("ent_entity_id_int", "entities", |t| {
        map_list(t, &["Entities"], &|e| e.put_int("id", 7))
    }),
    ("ent_entity_id_unknown", "entities", |t| {
        map_list(t, &["Entities"], &|e| e.put_string("id", "minecraft:zzz"))
    }),
    ("ent_entity_without_id", "entities", |t| {
        map_list(t, &["Entities"], &|e| {
            e.remove("id");
        })
    }),
    ("ent_no_data_version", "entities", |t| {
        t.remove("DataVersion");
    }),
    ("ent_old_data_version", "entities", |t| {
        t.put_int("DataVersion", 2000)
    }),
    ("ent_position_string", "entities", |t| {
        t.put_string("Position", "x")
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
        copy_dir(&world(folder), &source);
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
        // a misplaced chunk or an unreadable entity position saves its report as `debug/chunk-world-<second>-server.txt`:
        // the number of files follows the run time, so only their presence is compared
        if matches!(*name, "chunk_xpos_mismatch" | "ent_position_string") {
            for run in [&vanilla, &ours] {
                let debug = run.dir.join("debug");
                assert!(debug.is_dir(), "{}: no chunk report", debug.display());
                fs::remove_dir_all(debug).expect("remove the reports");
            }
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
        // a world of the 1.16.5 layout differs between two runs of the real server in its generated nether and end: only
        // the chunks that hold entities in the overworld are compared
        if folder == &"old_region" {
            let chunks = |run: &crate::world_start::Run| {
                entity_chunks(
                    &run.dir
                        .join("world/dimensions/minecraft/overworld/entities"),
                )
            };
            let (vanilla_chunks, our_chunks) = (chunks(&vanilla), chunks(&ours));
            if vanilla_chunks != our_chunks {
                findings.push(format!(
                    "chunks with entities: vanilla {vanilla_chunks:?}, ours {our_chunks:?}"
                ));
            }
        } else if vanilla.started && ours.started && *name != "chunk_sections_int" {
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
