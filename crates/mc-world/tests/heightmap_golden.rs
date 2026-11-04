//! Heightmap vs. the real vanilla `Heightmap` on a real `ProtoChunk` (`tests/fixtures/heightmap/HeightmapGolden.java`),
//! plus unit checks of `HeightmapType`.

mod height_status_common;

use height_status_common::TestChunk;
use mc_data::block::BlockState;
use mc_data::Block;
use mc_world::heightmap::{Heightmap, HeightmapChunk, HeightmapMap, HeightmapType, SizeMismatch};
use mc_world::status::ChunkStatus;

fn parse_longs(s: &str) -> Vec<i64> {
    s.split(',').map(|v| v.parse().unwrap()).collect()
}

/// `ProtoChunk.setBlockState` heightmap part.
fn set_block_state(chunk: &mut TestChunk, maps: &mut HeightmapMap, status: ChunkStatus, x: i32, y: i32, z: i32, state: BlockState) {
    let section = (y - chunk.min_y) / 16;
    let was_empty = chunk.section_has_only_air(section);
    if was_empty && state.block() == Block::AIR {
        return;
    }
    let i = chunk.idx(x, y, z);
    chunk.states[i] = state;
    let mut to_prime = Vec::new();
    for &ty in status.heightmaps_after() {
        match maps.get_mut(&ty) {
            None => to_prime.push(ty),
            Some(hm) => {
                hm.update(x, y, z, state, chunk);
            }
        }
    }
    if !to_prime.is_empty() {
        Heightmap::prime_heightmaps(chunk, maps, &to_prime);
    }
}

#[test]
fn matches_vanilla_heightmap_on_proto_chunk() {
    let text = include_str!("fixtures/heightmap/golden.txt");
    let mut chunk = TestChunk::new(0, 16);
    let mut maps = HeightmapMap::new();
    let mut status = ChunkStatus::Empty;
    let mut snapshot: Vec<HeightmapType> = Vec::new();
    let (mut ops, mut snaps, mut primes, mut cfgs) = (0, 0, 0, 0);
    let mut ended = false;
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "CFG" => {
                cfgs += 1;
                chunk = TestChunk::new(f[1].parse().unwrap(), f[2].parse().unwrap());
                maps = HeightmapMap::new();
                status = ChunkStatus::decode_str(f[3]).unwrap();
                snapshot.clear();
            }
            "OP" => {
                ops += 1;
                let (x, y, z, id) = (f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap(), f[4].parse().unwrap());
                set_block_state(&mut chunk, &mut maps, status, x, y, z, BlockState::from_id(id).unwrap());
            }
            "SNAP" => {
                snaps += 1;
                let ty = HeightmapType::from_serialization_key(f[1]).unwrap();
                snapshot.push(ty);
                assert_eq!(chunk.highest_section_position(), f[2].parse::<i32>().unwrap());
                let hm = maps.get(&ty).unwrap_or_else(|| panic!("{ty:?} not primed"));
                assert_eq!(hm.get_raw_data(), parse_longs(f[3]).as_slice(), "SNAP {ty:?} after {ops} ops (cfg {cfgs})");
            }
            "PRIME" => {
                primes += 1;
                let ty = HeightmapType::from_serialization_key(f[1]).unwrap();
                assert_eq!(chunk.highest_section_position(), f[2].parse::<i32>().unwrap());
                let mut fresh = HeightmapMap::new();
                Heightmap::prime_heightmaps(&chunk, &mut fresh, &HeightmapType::ALL);
                assert_eq!(fresh[&ty].get_raw_data(), parse_longs(f[3]).as_slice(), "PRIME {ty:?} (cfg {cfgs})");
            }
            "END" => ended = true,
            _ => panic!("bad line {line}"),
        }
    }
    assert!(ended);
    assert_eq!((cfgs, ops, snaps, primes), (6, 6000, 80, 36), "counts");
}

#[test]
fn types_ids_keys_flags() {
    use HeightmapType::*;
    let keys = ["WORLD_SURFACE_WG", "WORLD_SURFACE", "OCEAN_FLOOR_WG", "OCEAN_FLOOR", "MOTION_BLOCKING", "MOTION_BLOCKING_NO_LEAVES"];
    for (i, t) in HeightmapType::ALL.into_iter().enumerate() {
        assert_eq!(t.id() as usize, i);
        assert_eq!(HeightmapType::from_id(i as i32), t);
        assert_eq!(t.serialization_key(), keys[i]);
        assert_eq!(HeightmapType::from_serialization_key(keys[i]), Some(t));
    }
    // ByIdMap.continuous(ZERO): out of range -> id 0
    assert_eq!(HeightmapType::from_id(6), WorldSurfaceWg);
    assert_eq!(HeightmapType::from_id(-1), WorldSurfaceWg);
    assert_eq!(HeightmapType::from_serialization_key("world_surface"), None);
    // Usage: WORLD_SURFACE_WG/OCEAN_FLOOR_WG worldgen, OCEAN_FLOOR live world, rest client
    let client: Vec<_> = HeightmapType::ALL.into_iter().filter(|t| t.send_to_client()).collect();
    assert_eq!(client, [WorldSurface, MotionBlocking, MotionBlockingNoLeaves]);
    let keep: Vec<_> = HeightmapType::ALL.into_iter().filter(|t| t.keep_after_worldgen()).collect();
    assert_eq!(keep, [WorldSurface, OceanFloor, MotionBlocking, MotionBlockingNoLeaves]);
}

#[test]
fn predicates_match_vanilla_definitions() {
    use HeightmapType::*;
    let water = Block::WATER.default_state();
    let stone = Block::STONE.default_state();
    let leaves = Block::OAK_LEAVES.default_state();
    let air = Block::AIR.default_state();
    let cave_air = Block::CAVE_AIR.default_state();
    let grass = Block::SHORT_GRASS.default_state();
    // WORLD_SURFACE: !isAir (cave air is air too)
    assert!(WorldSurface.is_opaque(stone) && WorldSurface.is_opaque(water) && WorldSurface.is_opaque(grass));
    assert!(!WorldSurface.is_opaque(air) && !WorldSurface.is_opaque(cave_air));
    // OCEAN_FLOOR: BLOCKS_MOTION_IN_HEIGHTMAP only (water/grass not)
    assert!(OceanFloor.is_opaque(stone) && OceanFloor.is_opaque(leaves));
    assert!(!OceanFloor.is_opaque(water) && !OceanFloor.is_opaque(grass));
    // MOTION_BLOCKING: tag or fluid
    assert!(MotionBlocking.is_opaque(water) && MotionBlocking.is_opaque(leaves) && !MotionBlocking.is_opaque(grass));
    // NO_LEAVES: leaves are not in BLOCKS_MOTION_IN_HEIGHTMAP_NO_LEAVES
    assert!(!MotionBlockingNoLeaves.is_opaque(leaves) && MotionBlockingNoLeaves.is_opaque(stone) && MotionBlockingNoLeaves.is_opaque(water));
}

#[test]
fn storage_shape_and_raw_data() {
    let mut hm = Heightmap::new(-64, 384, HeightmapType::MotionBlocking);
    // ceillog2(385) = 9 bits -> 7 values per long -> 37 longs
    assert_eq!(hm.get_raw_data().len(), 37);
    assert_eq!(Heightmap::new(0, 256, HeightmapType::OceanFloor).get_raw_data().len(), 37); // ceillog2(257) = 9
    assert_eq!(Heightmap::new(0, 255, HeightmapType::OceanFloor).get_raw_data().len(), 32); // ceillog2(256) = 8 bits
    assert_eq!(hm.get_first_available(3, 4), -64);
    hm.set_height(3, 4, 70);
    assert_eq!(hm.get_first_available(3, 4), 70);
    assert_eq!(hm.get_highest_taken(3, 4), 69);

    let raw = hm.get_raw_data().to_vec();
    let mut other = Heightmap::new(-64, 384, HeightmapType::MotionBlocking);
    assert_eq!(other.set_raw_data(&raw), Ok(()));
    assert_eq!(other.get_first_available(3, 4), 70);
    let before = other.get_raw_data().to_vec();
    let err = other.set_raw_data(&raw[1..]).unwrap_err();
    assert_eq!(err, SizeMismatch { expected: 37, got: 36 });
    assert_eq!(err.message(mc_core::pos::ChunkPos::new(2, -3)), "Ignoring heightmap data for chunk [2, -3], size does not match; expected: 37, got: 36");
    assert_eq!(other.get_raw_data(), before.as_slice()); // untouched on mismatch
}

#[test]
fn update_rules() {
    let mut chunk = TestChunk::new(0, 64);
    let stone = Block::STONE.default_state();
    let air = Block::AIR.default_state();
    let mut hm = Heightmap::new(0, 64, HeightmapType::WorldSurface);
    let put = |chunk: &mut TestChunk, y, s| {
        let i = chunk.idx(1, y, 2);
        chunk.states[i] = s;
    };
    put(&mut chunk, 10, stone);
    assert!(hm.update(1, 10, 2, stone, &chunk));
    assert_eq!(hm.get_first_available(1, 2), 11);
    // y <= first_available - 2 (11 - 2 = 9): early out, even though the state is opaque
    put(&mut chunk, 8, stone);
    assert!(!hm.update(1, 8, 2, stone, &chunk));
    put(&mut chunk, 9, stone);
    assert!(!hm.update(1, 9, 2, stone, &chunk));
    // remove the top: scan down finds y=9
    put(&mut chunk, 10, air);
    assert!(hm.update(1, 10, 2, air, &chunk));
    assert_eq!(hm.get_first_available(1, 2), 10);
    // remove 9: stone at 8 remains
    put(&mut chunk, 9, air);
    assert!(hm.update(1, 9, 2, air, &chunk));
    assert_eq!(hm.get_first_available(1, 2), 9);
    // remove 8: nothing left, falls to min_y
    put(&mut chunk, 8, air);
    assert!(hm.update(1, 8, 2, air, &chunk));
    assert_eq!(hm.get_first_available(1, 2), 0);
    // non-opaque above the top: nothing
    assert!(!hm.update(1, 30, 2, air, &chunk));
}
