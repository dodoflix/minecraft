//! Forced chunks persist through the `chunk_tickets` saved data and keep chunks loaded after a restart.

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_game::server::level::world_gen::{ChunkGeneratorBackend, WorldGenRegion};
use mc_game::server::level::{ServerLevel, ServerLevelParams};
use mc_game::server::shared::ServerShared;
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct VoidGenerator;

impl ChunkGeneratorBackend for VoidGenerator {
    fn type_name_for_data_fixer(&self) -> Option<Identifier> {
        None
    }
    fn create_biomes(&self, _chunk: &mut ProtoChunk, _region: &WorldGenRegion) {}
    fn build_terrain(&self, _chunk: &mut ProtoChunk, _region: &WorldGenRegion) {}
    fn get_base_height(&self, _x: i32, _z: i32, _ty: mc_world::heightmap::HeightmapType, height: LevelHeight) -> i32 {
        height.min_y
    }
}

fn open(dir: &Path) -> ServerLevel {
    let (shared, _rx) = ServerShared::new();
    let biomes = BiomeRegistry::new(vec!["minecraft:plains".to_string()]);
    ServerLevel::new(ServerLevelParams {
        shared,
        dimension: Identifier::parse("minecraft:overworld").unwrap(),
        level_id: "tickets".into(),
        dimension_dir: dir.to_path_buf(),
        level_height: LevelHeight { min_y: -64, height: 384 },
        sea_level: 63,
        has_sky_light: true,
        factory: Arc::new(PalettedContainerFactory::create(biomes).unwrap()),
        generator: Arc::new(VoidGenerator),
        generate_structures: false,
        hooks: Arc::new(mc_game::server::level::world_gen::NoWorldGenHooks),
        seed: 0,
        is_known_structure: Arc::new(|_| false),
        view_distance: 4,
        simulation_distance: 4,
        sync_writes: false,
        is_debug: false,
        level_data: Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(mc_game::server::level::level_data::default_world_data())),
    })
}

fn run_until(level: &mut ServerLevel, mut done: impl FnMut(&mut ServerLevel) -> bool) {
    let start = Instant::now();
    while !done(level) {
        if !level.poll_chunk_task() {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(start.elapsed() < Duration::from_secs(30), "timed out");
    }
}

#[test]
fn forced_chunks_survive_a_restart() {
    let dir = std::env::temp_dir().join(format!("mc-chunk-tickets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let pos = ChunkPos::new(3, 4);
    {
        let mut level = open(&dir);
        assert!(level.chunk_source.update_chunk_forced(pos, true));
        assert!(!level.chunk_source.update_chunk_forced(pos, true), "second add is not a change");
        assert!(level.chunk_source.get_force_loaded_chunks().contains(pos.pack()));
        run_until(&mut level, |l| l.chunk_now(3, 4).is_some());
        level.chunk_source.deactivate_tickets_on_closing();
        level.close();
    }
    assert!(dir.join("data/minecraft/chunk_tickets.dat").is_file());
    {
        let mut level = open(&dir);
        assert!(level.chunk_now(3, 4).is_none());
        level.chunk_source.activate_all_deactivated_tickets();
        assert!(level.chunk_source.get_force_loaded_chunks().contains(pos.pack()));
        run_until(&mut level, |l| l.chunk_now(3, 4).is_some());
        assert!(level.chunk_source.update_chunk_forced(pos, false));
        run_until(&mut level, |l| {
            l.tick_chunk_source(&|| true, false, true);
            l.chunk_source.get_loaded_chunks_count() == 0
        });
        level.close();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
