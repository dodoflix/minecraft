//! `GenLevel` rules that the random-script golden test (`gen_level_golden.rs`) cannot pin: the exact log messages (the first
//! two literals are captured from the vanilla server classes by `GenLevelOracle`'s runs: `Util.logAndPauseIfInIde` output),
//! the crash on unavailable chunks, and read-only snapshot chunks (`ImposterProtoChunk(allowWrites = false)`).


use crate::common::feature_harness::*;
use crate::common::{SERIAL, init_logger, take_logs};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::Block;
use mc_game::server::level::world_gen::GenChunk;
use mc_game::world::level::levelgen::WorldGenLevel;
use mc_game::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_world::heightmap::HeightmapType;
use mc_world::status::ChunkStatus;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn out_of_zone_read_and_write_log_like_vanilla() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    init_logger();
    let mut synth = SynthRegion::new(7, -2, 0, ChunkStatus::Features, 2);
    take_logs();
    {
        let mut level = synth.level();
        // chunk [0, 0] is 2 chunks from the center: inside the read radius, outside the write radius 1
        level.get_block_state(BlockPos::new(0, 60, 5));
        assert!(!level.set_block(BlockPos::new(0, 81, 15), Block::STONE.default_state(), 3, 512));
        assert!(!level.ensure_can_write(BlockPos::new(-38, 85, 35)));
        level.set_currently_generating(Some(Box::new(|| "minecraft:ore_coal".to_string())));
        level.get_fluid_state(BlockPos::new(1, 60, 5));
        // the center and its neighbors are readable and writable silently
        level.get_block_state(BlockPos::new(-32, 60, 0));
        assert!(level.set_block(BlockPos::new(-17, 81, 15), Block::STONE.default_state(), 3, 512));
    }
    assert_eq!(
        take_logs(),
        [
            "ERROR: Detected unsafe terrain read during worldgen: reading from chunk [0, 0] while generating chunk [-2, 0] (distance: 2, write radius: 1), step: minecraft:features",
            "ERROR: Detected setBlock in a far chunk [0, 0], pos: BlockPos{x=0, y=81, z=15}, status: minecraft:features",
            "ERROR: Detected setBlock in a far chunk [-3, 2], pos: BlockPos{x=-38, y=85, z=35}, status: minecraft:features",
            "ERROR: Detected unsafe terrain read during worldgen: reading from chunk [0, 0] while generating chunk [-2, 0] (distance: 2, write radius: 1), step: minecraft:features, currently generating: minecraft:ore_coal",
        ]
    );
}

#[test]
fn unavailable_chunks_are_a_crash() {
    let mut synth = SynthRegion::new(9, 0, 0, ChunkStatus::Features, 1);
    let level = synth.level();
    // outside the lent set (vanilla: out of cache bounds)
    let crash = catch_unwind(AssertUnwindSafe(|| level.get_block_state(BlockPos::new(16 * 5, 60, 0)))).expect_err("unavailable chunk");
    let message = crash.downcast_ref::<String>().expect("panic message");
    assert!(message.contains("Requested chunk unavailable during world generation"), "{message}");
    assert!(message.contains("Requested chunk: 5, 0") && message.contains("[out of cache bounds]"), "{message}");
    // BIOMES status is not available at distance 2 of a FEATURES step (STRUCTURE_STARTS only): vanilla crashes too
    let mut synth = SynthRegion::new(9, 0, 0, ChunkStatus::Features, 2);
    let level = synth.level();
    let crash = catch_unwind(AssertUnwindSafe(|| level.get_noise_biome(2 * 4, 0, 0))).expect_err("status gated");
    let message = crash.downcast_ref::<String>().expect("panic message");
    assert!(message.contains("Maximum allowed status: minecraft:structure_starts"), "{message}");
}

#[test]
fn has_chunk_is_distance_based() {
    let mut synth = SynthRegion::new(9, 0, 0, ChunkStatus::Features, 2);
    let level = synth.level();
    // FEATURES depends on STRUCTURE_STARTS up to 8 chunks away
    assert!(level.has_chunk(8, -8));
    assert!(!level.has_chunk(9, 0));
}

#[test]
fn read_only_snapshot_chunks_drop_writes() {
    let mut synth = SynthRegion::new(11, 0, 0, ChunkStatus::Features, 1);
    let pos = ChunkPos::new(1, 0);
    let Some(GenChunk::Loaned(chunk)) = synth.region.take_chunk(pos) else { panic!("lent chunk") };
    synth.region.put_chunk(pos, GenChunk::Snapshot(chunk));
    let p = BlockPos::new(20, 90, 5);
    let before = synth.region.get_chunk(1, 0).unwrap().get_block_state(p);
    {
        let mut level = synth.level();
        // within the write zone: reported as written, but the imposter drops everything
        assert!(level.set_block(p, Block::CHEST.default_state(), 3, 512));
        level.mark_pos_for_post_processing(p);
        level.set_block_entity_nbt(mc_nbt::snbt::parse_compound("{id:\"minecraft:chest\",x:20,y:90,z:5}").unwrap());
        level.schedule_block_tick(p, Block::SAND, 1, mc_world::ticks::TickPriority::Normal);
        assert!(!level.has_scheduled_block_tick(p, Block::SAND));
        // WG heightmap types are answered by the final ones of the wrapped full chunk
        let _ = level.get_height_at(HeightmapType::OceanFloorWg, 20, 5);
    }
    let chunk = synth.region.get_chunk(1, 0).unwrap();
    assert_eq!(chunk.get_block_state(p), before);
    assert!(chunk.access.block_entities.is_empty());
    assert!(chunk.access.post_processing.iter().all(Option::is_none));
    assert_eq!(chunk.block_ticks.scheduled_ticks().len(), 0);
}

#[test]
fn block_entity_placeholder_is_replaced_and_removed() {
    let mut synth = SynthRegion::new(13, 0, 0, ChunkStatus::Features, 1);
    let p = BlockPos::new(3, 90, 4);
    let mut level = synth.level();
    assert!(level.set_block(p, Block::CHEST.default_state(), 3, 512));
    assert_eq!(level.get_block_entity_nbt(p).unwrap().get_string("id"), Some("DUMMY"));
    level.set_block_entity_nbt(mc_nbt::snbt::parse_compound("{id:\"minecraft:chest\",x:3,y:90,z:4}").unwrap());
    assert_eq!(level.get_block_entity_nbt(p).unwrap().get_string("id"), Some("minecraft:chest"));
    assert!(level.set_block(p, Block::STONE.default_state(), 3, 512));
    assert!(level.get_block_entity_nbt(p).is_none());
}
