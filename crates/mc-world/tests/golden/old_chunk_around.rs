//! `IOWorker.isOldChunkAround` (through `SimpleRegionStorage`): the script of
//! `fixtures/region/old_chunk_around.script` replayed against vanilla's real storage
//! (`fixtures/region/OldChunkAroundTool.java`, answers in `old_chunk_around.expected`) and ours.

use crate::region_common::*;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_world::region::SimpleRegionStorage;

fn chunk(version: &str, blend: &str) -> CompoundTag {
    let mut tag = CompoundTag::new();
    match version {
        "none" => {}
        "long" => tag.put_long("DataVersion", 5023),
        v => tag.put_int("DataVersion", v.parse().unwrap()),
    }
    match blend {
        "blend" => {
            let mut blending = CompoundTag::new();
            blending.put_int("min_section", -4);
            blending.put_int("max_section", 20);
            tag.put("blending_data", blending);
        }
        "blendint" => tag.put_int("blending_data", 1),
        _ => {}
    }
    tag
}

#[test]
fn is_old_chunk_around_equals_vanilla() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SimpleRegionStorage::new(info(), dir.path(), false);
    let script = std::fs::read_to_string(fixtures().join("old_chunk_around.script")).unwrap();
    let mut answers = String::new();
    for line in script.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let pos = |i: usize| ChunkPos::new(f[i].parse().unwrap(), f[i + 1].parse().unwrap());
        match f.first().copied() {
            None => {}
            Some(c) if c.starts_with('#') => {}
            Some("W") => storage.write(&pos(1), Some(chunk(f[3], f[4]))),
            Some("D") => storage.write(&pos(1), None),
            Some("S") => storage.synchronize(true).unwrap(),
            Some("Q") => {
                let old = storage.is_old_chunk_around(pos(1), f[3].parse().unwrap());
                answers.push_str(&format!("Q {} {} {} {old}\n", f[1], f[2], f[3]));
            }
            Some(other) => panic!("unknown script line: {other}"),
        }
    }
    let expected = std::fs::read_to_string(fixtures().join("old_chunk_around.expected")).unwrap();
    let (ours, vanilla): (Vec<&str>, Vec<&str>) =
        (answers.lines().collect(), expected.lines().collect());
    assert_eq!(ours.len(), vanilla.len());
    let differing: Vec<_> = ours.iter().zip(&vanilla).filter(|(a, b)| a != b).collect();
    assert!(differing.is_empty(), "differs from vanilla: {differing:?}");
}

#[test]
fn a_chunk_that_cannot_be_read_is_not_old() {
    let dir = tempfile::tempdir().unwrap();
    let mut storage = SimpleRegionStorage::new(info(), dir.path(), false);
    storage.write(&ChunkPos::new(2, 2), Some(chunk("100", "-")));
    storage.synchronize(true).unwrap();
    storage.close().unwrap();
    // `Failed to bulk scan chunk` / `Failed to scan chunk`: the chunk's compressed data is damaged, its bit stays clear
    let file = dir.path().join("r.0.0.mca");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes[8192 + 5..8192 + 13].fill(0xFF);
    std::fs::write(&file, bytes).unwrap();
    let mut storage = SimpleRegionStorage::new(info(), dir.path(), false);
    assert!(storage.scan_chunk(&ChunkPos::new(2, 2)).is_err());
    assert!(!storage.is_old_chunk_around(ChunkPos::new(2, 2), 0));
}
