//! Loads the vanilla reference world (`mc_testkit::worldref`) through `ServerLevel`/`ChunkMap` (loading pyramid, worker threads,
//! light seam), compares the loaded chunks with the NBT on disk and round-trips saved chunks.

use mc_core::ident::Identifier;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_game::server::level::{ChunkMap, ServerLevel, ServerLevelParams, Ticket, TicketType};
use mc_game::server::shared::ServerShared;
use mc_game::world::level::{BlockGetter, LevelReader};
use mc_nbt::CompoundTag;
use mc_world::chunk::LevelHeight;
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::region::{RegionFileStorage, RegionStorageInfo};
use mc_world::serial::SerializableChunkData;
use mc_world::status::ChunkStatus;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn world_dimension_dir() -> PathBuf {
    mc_testkit::worldref()
        .expect("reference world")
        .join("dimensions/minecraft/overworld")
}

fn factory() -> Arc<PalettedContainerFactory> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ref/generated/data/minecraft/worldgen/biome");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            format!(
                "minecraft:{}",
                e.unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .trim_end_matches(".json")
            )
        })
        .collect();
    names.sort();
    Arc::new(PalettedContainerFactory::create(BiomeRegistry::new(names)).unwrap())
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// Positions and NBT of every chunk in the `.mca` files of `folder`.
fn read_all(folder: &Path) -> Vec<(ChunkPos, CompoundTag)> {
    let mut regions = Vec::new();
    for e in std::fs::read_dir(folder).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        if let Some(rest) = name.strip_prefix("r.").and_then(|r| r.strip_suffix(".mca")) {
            let (x, z) = rest.split_once('.').unwrap();
            regions.push((x.parse::<i32>().unwrap(), z.parse::<i32>().unwrap()));
        }
    }
    regions.sort();
    let mut storage = RegionFileStorage::new(
        RegionStorageInfo::new(
            "test",
            Identifier::parse("minecraft:overworld").unwrap(),
            "chunk",
        ),
        folder,
        false,
    );
    let mut out = Vec::new();
    for (rx, rz) in regions {
        for lz in 0..32 {
            for lx in 0..32 {
                let pos = ChunkPos::new(rx * 32 + lx, rz * 32 + lz);
                if let Some(tag) = storage.read(&pos).unwrap() {
                    out.push((pos, tag));
                }
            }
        }
    }
    out
}

fn open_level(dir: &Path) -> ServerLevel {
    let (shared, _rx) = ServerShared::new();
    ServerLevel::new(ServerLevelParams {
        shared,
        dimension: Identifier::parse("minecraft:overworld").unwrap(),
        level_id: "test".to_string(),
        dimension_dir: dir.to_path_buf(),
        level_height: LevelHeight {
            min_y: -64,
            height: 384,
        },
        sea_level: 63,
        has_sky_light: true,
        factory: factory(),
        generator: Arc::new(VoidGenerator),
        generate_structures: false,
        hooks: Arc::new(mc_game::server::level::world_gen::NoWorldGenHooks),
        seed: 0,
        is_known_structure: Arc::new(|_| true),
        view_distance: 10,
        simulation_distance: 10,
        sync_writes: false,
        is_debug: false,
        level_data: Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
            mc_game::server::level::level_data::default_world_data(),
        )),
    })
}

/// Chunks that must be generated (neighbours of the world edge) come out empty.
struct VoidGenerator;

impl mc_game::server::level::world_gen::ChunkGeneratorBackend for VoidGenerator {
    fn type_name_for_data_fixer(&self) -> Option<Identifier> {
        None
    }
    fn create_biomes(
        &self,
        _chunk: &mut mc_world::chunk::ProtoChunk,
        _region: &mc_game::server::level::world_gen::WorldGenRegion,
    ) {
    }
    fn build_terrain(
        &self,
        _chunk: &mut mc_world::chunk::ProtoChunk,
        _region: &mc_game::server::level::world_gen::WorldGenRegion,
    ) {
    }
    fn get_base_height(
        &self,
        _x: i32,
        _z: i32,
        _ty: mc_world::heightmap::HeightmapType,
        height: LevelHeight,
    ) -> i32 {
        height.min_y
    }
}

fn full_chunks(dir: &Path) -> Vec<(ChunkPos, CompoundTag)> {
    read_all(&dir.join("region"))
        .into_iter()
        .filter(|(_, t)| t.get_string("Status") == Some("minecraft:full"))
        .collect()
}

fn sections_nbt(
    data: &SerializableChunkData,
    factory: &PalettedContainerFactory,
) -> Vec<(i32, CompoundTag, CompoundTag)> {
    data.section_data
        .iter()
        .filter_map(|s| {
            s.chunk_section.as_ref().map(|c| {
                (
                    s.y,
                    c.states()
                        .to_nbt(&mc_world::palette::BlockStateCodec)
                        .unwrap(),
                    c.biomes().to_nbt(&factory.biomes).unwrap(),
                )
            })
        })
        .collect()
}

#[test]
#[ignore = "oracle: needs the vanilla reference world (generated by mc_testkit::worldref)"]
fn loads_vanilla_chunks_through_the_chunk_map() {
    let src = world_dimension_dir();
    let tmp = std::env::temp_dir().join(format!("mc-chunkmap-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    copy_dir(&src, &tmp);
    let fulls = full_chunks(&tmp);
    assert!(
        fulls.len() > 100,
        "expected a populated world, got {}",
        fulls.len()
    );
    let factory = factory();
    let mut level = open_level(&tmp);
    // Ask for a handful of chunks spread over the world; neighbours that are not full yet are generated (void).
    let sample: Vec<&(ChunkPos, CompoundTag)> = fulls.iter().step_by(37).take(6).collect();
    for (pos, tag) in &sample {
        let chunk = level.chunk_blocking_mut(pos.x, pos.z);
        let parsed = SerializableChunkData::parse(
            LevelHeight {
                min_y: -64,
                height: 384,
            },
            &factory,
            tag,
        )
        .unwrap()
        .unwrap();
        let expected = sections_nbt(&parsed, &factory);
        let got: Vec<(i32, CompoundTag, CompoundTag)> = (0..chunk.data.access.sections.len())
            .map(|i| {
                let section_y = chunk
                    .data
                    .access
                    .level_height
                    .section_y_from_section_index(i as i32);
                let s = &chunk.data.access.sections[i];
                (
                    section_y,
                    s.states()
                        .to_nbt(&mc_world::palette::BlockStateCodec)
                        .unwrap(),
                    s.biomes().to_nbt(&factory.biomes).unwrap(),
                )
            })
            .filter(|(y, _, _)| expected.iter().any(|(ey, _, _)| ey == y))
            .collect();
        assert_eq!(got, expected, "sections of chunk {pos}");
        assert!(level.chunk_source.has_chunk(pos.x, pos.z));
        // The stored light layers were queued into the light engine when the chunk loaded.
        for section in &parsed.section_data {
            for (x, y, z) in [(0, 0, 0), (3, 7, 12), (15, 15, 15), (8, 2, 5)] {
                let bp = BlockPos::new(pos.x * 16 + x, section.y * 16 + y, pos.z * 16 + z);
                if let Some(layer) = &section.block_light {
                    assert_eq!(
                        level.get_brightness(mc_game::world::level::LightLayer::Block, bp),
                        layer.get(x, y, z),
                        "block light at {bp:?}"
                    );
                }
                if let Some(layer) = &section.sky_light {
                    assert_eq!(
                        level.get_brightness(mc_game::world::level::LightLayer::Sky, bp),
                        layer.get(x, y, z),
                        "sky light at {bp:?}"
                    );
                }
            }
        }
    }
    // BlockGetter reads through the chunk source.
    let (pos, _) = sample[0];
    let state = level.get_block_state(BlockPos::new(pos.x * 16, 63, pos.z * 16));
    let chunk = level.chunk_now(pos.x, pos.z).unwrap();
    assert_eq!(
        state,
        chunk
            .data
            .get_block_state(BlockPos::new(pos.x * 16, 63, pos.z * 16))
    );
    level.close();
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[ignore = "oracle: needs the vanilla reference world (generated by mc_testkit::worldref)"]
fn saved_chunks_round_trip() {
    let src = world_dimension_dir();
    let tmp = std::env::temp_dir().join(format!("mc-chunkmap-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    copy_dir(&src, &tmp);
    let fulls = full_chunks(&tmp);
    let (pos, _) = fulls[fulls.len() / 2].clone();
    let target = BlockPos::new(pos.x * 16 + 3, 100, pos.z * 16 + 5);
    let stone = mc_data::block::Block::GOLD_BLOCK.default_state();
    {
        let mut level = open_level(&tmp);
        let chunk = level.chunk_blocking_mut(pos.x, pos.z);
        chunk
            .data
            .set_block_state_raw(target, stone)
            .expect("block changed");
        chunk.data.access.mark_unsaved();
        level.close();
    }
    {
        let mut level = open_level(&tmp);
        let chunk = level.chunk_blocking_mut(pos.x, pos.z);
        assert_eq!(chunk.data.get_block_state(target), stone);
        level.close();
    }
    // The file on disk is a normal full chunk.
    let tag = read_all(&tmp.join("region"))
        .into_iter()
        .find(|(p, _)| *p == pos)
        .unwrap()
        .1;
    assert_eq!(tag.get_string("Status"), Some("minecraft:full"));
    let _ = (
        ChunkStatus::Full,
        ChunkMap::size as fn(&ChunkMap) -> usize,
        Ticket::new(TicketType::UNKNOWN, 33),
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
