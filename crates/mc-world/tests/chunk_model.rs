//! Behaviour of the chunk data model (heights, proto/level chunk block updates, offsets, section counts).

mod chunk_common;

use chunk_common::factory;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::Block;
use mc_world::chunk::*;
use mc_world::heightmap::HeightmapType;
use mc_world::status::ChunkStatus;
use mc_world::upgrade_data::UpgradeData;

#[test]
fn level_height_matches_vanilla_dimensions() {
    let overworld = LevelHeight::new(-64, 384);
    assert_eq!((overworld.min_section_y(), overworld.max_section_y(), overworld.sections_count()), (-4, 19, 24));
    assert_eq!(overworld.section_index(-64), 0);
    assert_eq!(overworld.section_index(319), 23);
    assert!(overworld.is_outside_build_height(320) && overworld.is_outside_build_height(-65));
    let nether = LevelHeight::new(0, 256);
    assert_eq!((nether.min_section_y(), nether.max_section_y(), nether.sections_count()), (0, 15, 16));
    // BelowZeroRetrogen.UPGRADE_HEIGHT_ACCESSOR
    assert_eq!((LevelHeight::UPGRADE.min_y, LevelHeight::UPGRADE.max_y()), (-64, -1));
}

#[test]
fn offset_coordinates_round_trip() {
    let pos = BlockPos::new(-17, 70, 5);
    let packed = pack_offset_coordinates(pos);
    assert_eq!(packed, (15 | (70 & 15) << 4 | 5 << 8) as i16);
    let un = unpack_offset_coordinates(packed, 4, ChunkPos::new(-2, 0));
    assert_eq!((un.x(), un.y(), un.z()), (-17, 64 + (70 & 15), 5));
}

#[test]
fn level_chunk_updates_heightmaps_and_section_counts() {
    let factory = factory();
    let h = LevelHeight::new(-64, 384);
    let mut chunk = LevelChunk::empty(h, &factory, ChunkPos::new(3, -2));
    let pos = BlockPos::new(3 * 16 + 4, 70, -2 * 16 + 9);
    let stone = Block::STONE.default_state();
    // air into an empty section is a no-op
    assert!(chunk.set_block_state_raw(pos, Block::AIR.default_state()).is_none());
    let r = chunk.set_block_state_raw(pos, stone).unwrap();
    assert!(r.section_was_empty && !r.section_is_empty && r.old_state == Block::AIR.default_state());
    assert_eq!(chunk.get_block_state(pos), stone);
    assert_eq!(chunk.access.get_height(false, HeightmapType::MotionBlocking, 4, 9), 70);
    assert_eq!(chunk.access.get_height(false, HeightmapType::WorldSurface, 4, 9), 70);
    // same state again: nothing happens
    assert!(chunk.set_block_state_raw(pos, stone).is_none());
    // water counts as motion blocking but not as ocean floor
    let above = pos.above();
    chunk.set_block_state_raw(above, Block::WATER.default_state()).unwrap();
    assert_eq!(chunk.access.get_height(false, HeightmapType::MotionBlocking, 4, 9), 71);
    assert_eq!(chunk.access.get_height(false, HeightmapType::OceanFloor, 4, 9), 70);
    assert!(chunk.get_fluid_state(pos.x(), above.y(), pos.z()).amount() > 0);
    // removing everything resets to min_y - 1
    chunk.set_block_state_raw(above, Block::AIR.default_state()).unwrap();
    chunk.set_block_state_raw(pos, Block::AIR.default_state()).unwrap();
    assert_eq!(chunk.access.get_height(false, HeightmapType::MotionBlocking, 4, 9), -65);
    assert!(chunk.access.get_section(chunk.access.level_height.section_index(70) as usize).has_only_air());
}

#[test]
fn proto_chunk_primes_missing_heightmaps_and_respects_status() {
    let factory = factory();
    let h = LevelHeight::new(0, 256);
    let mut proto = ProtoChunk::new(ChunkPos::new(0, 0), UpgradeData::empty(), h, &factory, None);
    assert_eq!(proto.get_block_state(BlockPos::new(1, 300, 1)), Block::VOID_AIR.default_state());
    assert_eq!(proto.set_block_state(BlockPos::new(1, 300, 1), Block::STONE.default_state(), None), Block::VOID_AIR.default_state());
    proto.set_persisted_status(ChunkStatus::Terrain);
    let pos = BlockPos::new(1, 20, 2);
    proto.set_block_state(pos, Block::STONE.default_state(), None);
    // Terrain's heightmapsAfter = the worldgen heightmaps; they were primed on first use
    for ty in ChunkStatus::Terrain.heightmaps_after() {
        assert!(proto.access.has_primed_heightmap(*ty));
        assert_eq!(proto.access.get_height(true, *ty, 1, 2), 20);
    }
    proto.mark_pos_for_post_processing(pos);
    assert_eq!(proto.access.post_processing[1].as_deref(), Some(&[pack_offset_coordinates(pos)][..]));
    // below-zero retrogen is cleared once the target status is reached
    proto.set_below_zero_retrogen(mc_world::below_zero_retrogen::BelowZeroRetrogen::parse(&mc_nbt::Tag::Compound(
        mc_nbt::CompoundTag::new().with("target_status", "minecraft:features"),
    )));
    assert!(proto.is_upgrading());
    proto.set_persisted_status(ChunkStatus::Features);
    assert!(!proto.is_upgrading());
}

#[test]
fn full_proto_to_level_chunk_keeps_data() {
    let factory = factory();
    let h = LevelHeight::new(0, 256);
    let mut proto = ProtoChunk::new(ChunkPos::new(2, 2), UpgradeData::empty(), h, &factory, None);
    proto.set_persisted_status(ChunkStatus::Full);
    proto.set_block_state(BlockPos::new(33, 10, 34), Block::DIRT.default_state(), None);
    proto.access.set_inhabited_time(77);
    proto.access.set_light_correct(true);
    proto.access.add_reference_for_structure("minecraft:village_plains".into(), 5);
    let chunk = LevelChunk::from_proto(proto, &factory);
    assert_eq!(chunk.get_block_state(BlockPos::new(33, 10, 34)), Block::DIRT.default_state());
    assert_eq!(chunk.access.inhabited_time(), 77);
    assert!(chunk.access.is_light_correct() && chunk.access.is_unsaved());
    assert!(chunk.access.structures.references["minecraft:village_plains"].contains(&5));
    let mut chunk = chunk;
    assert_eq!(chunk.access.get_height(false, HeightmapType::MotionBlocking, 1, 2), 10);
}
