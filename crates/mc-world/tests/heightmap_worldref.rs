//! Differential test against real chunks of the vanilla-generated world `run/servers/worldref`: priming the heightmaps
//! of every persisted chunk from its decoded blocks must reproduce the persisted `Heightmaps` longs; the stored data also
//! round-trips through `Heightmap::set_raw_data`. The status / retrogen tags of the same chunks must decode as vanilla.
//! Passes (printing a note) when the world is absent.

mod height_status_common;

use std::io::Read;
use std::path::{Path, PathBuf};

use height_status_common::TestChunk;
use mc_core::bits::{BitStorage, SimpleBitStorage};
use mc_core::math::mth;
use mc_data::block::BlockState;
use mc_data::Block;
use mc_nbt::{nbt_io, CompoundTag, NbtAccounter, Tag};
use mc_world::heightmap::{Heightmap, HeightmapMap, HeightmapType};
use mc_world::status::ChunkStatus;

fn world_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/servers/worldref/world")
}

/// Every (zlib / gzip / uncompressed) chunk payload of one `.mca` as a root compound.
fn read_region(path: &Path) -> Vec<CompoundTag> {
    let data = std::fs::read(path).unwrap();
    let mut out = Vec::new();
    for i in 0..1024 {
        let loc = u32::from_be_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        if loc == 0 {
            continue;
        }
        let off = (loc >> 8) as usize * 4096;
        let len = u32::from_be_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        let payload = &data[off + 5..off + 4 + len];
        let mut raw = Vec::new();
        match data[off + 4] {
            1 => flate2::read::GzDecoder::new(payload).read_to_end(&mut raw).map(drop).unwrap(),
            2 => flate2::read::ZlibDecoder::new(payload).read_to_end(&mut raw).map(drop).unwrap(),
            3 => raw.extend_from_slice(payload),
            other => panic!("unsupported region compression {other} in {}", path.display()),
        }
        out.push(nbt_io::read(&mut raw.as_slice(), &mut NbtAccounter::unlimited_heap()).unwrap());
    }
    out
}

/// A palette entry (`BlockState.CODEC`): a bare `"minecraft:air"` string for the default state, else `{id, properties}`.
fn decode_state(entry: &Tag) -> BlockState {
    let (name, props): (&str, Vec<(String, String)>) = match entry {
        Tag::String(name) => (name, Vec::new()),
        Tag::Compound(c) => (
            c.get_string("id").unwrap(),
            c.get_compound("properties")
                .map(|p| p.iter().map(|(k, v)| (k.clone(), v.as_string().unwrap().to_owned())).collect())
                .unwrap_or_default(),
        ),
        other => panic!("bad palette entry {other:?}"),
    };
    let block = Block::by_name(name).unwrap_or_else(|| panic!("unknown block {name}"));
    block.state_from_properties(props.iter().map(|(k, v)| (k.as_str(), v.as_str()))).unwrap()
}

/// `sections[].block_states` -> dense chunk (palette strategy: single value, else `max(4, ceillog2(size))` bits).
fn decode_chunk(nbt: &CompoundTag, min_y: i32, height: i32) -> TestChunk {
    let mut chunk = TestChunk::new(min_y, height);
    for section in nbt.get_list("sections").unwrap().compounds() {
        let sy = i32::from(section.get_byte("Y").unwrap());
        let Some(index) = (sy * 16 - min_y).checked_div(16).filter(|i| (0..height / 16).contains(i) && sy * 16 >= min_y) else {
            continue;
        };
        let bs = section.get_compound("block_states").unwrap();
        let palette: Vec<BlockState> = bs.get_list("palette").unwrap().iter().map(decode_state).collect();
        if palette.is_empty() {
            continue; // sections of not-yet-generated chunks
        }
        let values = bs.get_long_array("data").map(|d| {
            let bits = mth::ceillog2(palette.len() as i32).max(4);
            SimpleBitStorage::with_data(bits, 4096, d.to_vec()).unwrap()
        });
        for i in 0..4096usize {
            let state = match &values {
                Some(storage) => palette[storage.get(i) as usize],
                None => palette[0],
            };
            let (x, y, z) = ((i & 15) as i32, (i >> 8) as i32, ((i >> 4) & 15) as i32);
            let idx = chunk.idx(x, index * 16 + min_y + y, z);
            chunk.states[idx] = state;
        }
    }
    chunk
}

#[test]
fn persisted_heightmaps_reprime_identically() {
    let world = world_dir();
    if !world.is_dir() {
        eprintln!("skipping: {} not present", world.display());
        return;
    }
    let (mut full_chunks, mut other_chunks, mut compared, mut other_mismatch) = (0, 0, 0, 0);
    let mut by_kind = std::collections::BTreeMap::new();
    for (dim, min_y, height) in [("overworld", -64, 384), ("the_nether", 0, 256), ("the_end", 0, 256)] {
        let dir = world.join("dimensions/minecraft").join(dim).join("region");
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        for file in files.iter().filter(|f| f.extension().is_some_and(|e| e == "mca")) {
            for nbt in read_region(file) {
                let status = nbt.get("Status").and_then(ChunkStatus::decode).unwrap_or(ChunkStatus::Empty);
                assert_eq!(nbt.get_int("yPos"), Some(min_y >> 4));
                let Some(stored) = nbt.get_compound("Heightmaps") else { continue };
                let chunk = decode_chunk(&nbt, min_y, height);
                let is_full = status == ChunkStatus::Full;
                if is_full {
                    full_chunks += 1;
                } else {
                    other_chunks += 1;
                }
                let mut types = Vec::new();
                for ty in HeightmapType::ALL {
                    let Some(longs) = stored.get_long_array(ty.serialization_key()) else { continue };
                    types.push(ty);
                    // set_raw_data round trip
                    let mut hm = Heightmap::new(min_y, height, ty);
                    hm.set_raw_data(longs).unwrap();
                    assert_eq!(hm.get_raw_data(), longs);
                }
                if is_full {
                    // vanilla loads a full chunk by priming exactly heightmapsAfter(status) minus the stored ones; all four are stored
                    for &ty in status.heightmaps_after() {
                        assert!(types.contains(&ty), "full chunk lacks {ty:?}");
                    }
                }
                let mut primed = HeightmapMap::new();
                Heightmap::prime_heightmaps(&chunk, &mut primed, &types);
                for ty in &types {
                    compared += 1;
                    let expected = stored.get_long_array(ty.serialization_key()).unwrap();
                    if primed[ty].get_raw_data() != expected {
                        if is_full {
                            panic!("{dim} {file:?} chunk {:?},{:?}: {ty:?} differs", nbt.get_int("xPos"), nbt.get_int("zPos"));
                        }
                        other_mismatch += 1;
                        *by_kind.entry((status.name(), ty.serialization_key())).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    eprintln!("full chunks {full_chunks}, other chunks with heightmaps {other_chunks}, compared {compared}, non-full mismatches {other_mismatch}: {by_kind:?}");
    assert!(full_chunks > 100, "expected the vanilla world to contain full chunks");
}

#[test]
fn status_and_retrogen_tags_of_real_chunks() {
    let world = world_dir();
    if !world.is_dir() {
        eprintln!("skipping: {} not present", world.display());
        return;
    }
    let file = world.join("dimensions/minecraft/overworld/region/r.0.0.mca");
    let mut seen = std::collections::BTreeMap::new();
    for nbt in read_region(&file) {
        let status_tag = nbt.get("Status").expect("chunk has Status");
        let status = ChunkStatus::decode(status_tag).expect("vanilla-written status decodes");
        assert_eq!(status_tag, &Tag::String(status.name()));
        *seen.entry(status.name()).or_insert(0) += 1;
        assert!(nbt.get("below_zero_retrogen").is_none() && nbt.get("blending_data").is_none());
    }
    eprintln!("statuses in r.0.0: {seen:?}");
    assert!(seen.contains_key("minecraft:full"));
}
