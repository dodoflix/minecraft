//! The chunk status pipeline's world generation stages: the hooks (`createStructures`, `createReferences`,
//! `applyBiomeDecoration`, `spawnOriginalMobs`) run on the worldgen thread in vanilla's `ChunkPyramid` order with the
//! step's region (read/write radius), and an exception in a step crashes the server with vanilla's report
//! (`ChunkMap.applyStep` / `GenerationChunkHolder.applyStep` -> `relayDelayCrash`).

use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_game::crash_report::ReportedException;
use mc_game::server::level::world_gen::{ChunkGeneratorBackend, WorldGenContext, WorldGenHooks, WorldGenRegion};
use mc_game::server::level::{ServerLevel, ServerLevelParams};
use mc_game::server::shared::ServerShared;
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::status::ChunkStatus;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stage {
    Starts,
    References { read: i32, write: i32 },
    Biomes,
    Terrain,
    Features { read: i32, write: i32 },
    Spawn,
}

type Log = Arc<Mutex<Vec<(Stage, ChunkPos, std::thread::ThreadId)>>>;

struct Generator(Log);

impl ChunkGeneratorBackend for Generator {
    fn type_name_for_data_fixer(&self) -> Option<Identifier> {
        Identifier::parse("minecraft:noise").ok()
    }
    fn create_biomes(&self, chunk: &mut ProtoChunk, _region: &WorldGenRegion) {
        self.0.lock().unwrap().push((Stage::Biomes, chunk.pos(), std::thread::current().id()));
    }
    fn build_terrain(&self, chunk: &mut ProtoChunk, _region: &WorldGenRegion) {
        self.0.lock().unwrap().push((Stage::Terrain, chunk.pos(), std::thread::current().id()));
    }
    fn get_base_height(&self, _x: i32, _z: i32, _ty: mc_world::heightmap::HeightmapType, height: LevelHeight) -> i32 {
        height.min_y
    }
}

struct Hooks {
    log: Log,
    /// Panic in `applyBiomeDecoration` of this chunk.
    fail_features_at: Option<ChunkPos>,
}

impl Hooks {
    fn record(&self, stage: Stage, pos: ChunkPos) {
        self.log.lock().unwrap().push((stage, pos, std::thread::current().id()));
    }
}

impl WorldGenHooks for Hooks {
    fn create_structures(&self, _ctx: &WorldGenContext, chunk: &mut ProtoChunk) {
        self.record(Stage::Starts, chunk.pos());
    }
    fn create_references(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.record(Stage::References { read: region.read_radius, write: region.write_radius }, region.center);
    }
    fn apply_biome_decoration(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.record(Stage::Features { read: region.read_radius, write: region.write_radius }, region.center);
        if self.fail_features_at == Some(region.center) {
            panic!("boom");
        }
    }
    fn spawn_original_mobs(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.record(Stage::Spawn, region.center);
    }
}

fn open(dir: &std::path::Path, log: &Log, fail_features_at: Option<ChunkPos>) -> ServerLevel {
    let (shared, _rx) = ServerShared::new();
    ServerLevel::new(ServerLevelParams {
        shared,
        dimension: Identifier::parse("minecraft:overworld").unwrap(),
        level_id: "pipeline".into(),
        dimension_dir: dir.to_path_buf(),
        level_height: LevelHeight { min_y: -64, height: 384 },
        sea_level: 63,
        has_sky_light: true,
        factory: Arc::new(PalettedContainerFactory::create(BiomeRegistry::new(vec!["minecraft:plains".to_string()])).unwrap()),
        generator: Arc::new(Generator(log.clone())),
        generate_structures: true,
        hooks: Arc::new(Hooks { log: log.clone(), fail_features_at }),
        seed: 0,
        is_known_structure: Arc::new(|_| false),
        view_distance: 4,
        simulation_distance: 4,
        sync_writes: false,
        is_debug: false,
        level_data: Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(mc_game::server::level::level_data::default_world_data())),
    })
}

fn position(log: &[(Stage, ChunkPos, std::thread::ThreadId)], stage: impl Fn(&Stage) -> bool, pos: ChunkPos) -> usize {
    log.iter().position(|(s, p, _)| stage(s) && *p == pos).unwrap_or_else(|| panic!("stage missing for {pos}"))
}

#[test]
fn stages_run_in_chunk_pyramid_order_on_the_worldgen_thread() {
    let dir = std::env::temp_dir().join(format!("mc-pipeline-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Arc::default();
    let mut level = open(&dir, &log, None);
    let target = ChunkPos::new(3, -2);
    assert!(level.proto_chunk_blocking(target.x, target.z, ChunkStatus::Spawn, true).is_some());
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
    let log = log.lock().unwrap().clone();

    let game_thread = std::thread::current().id();
    assert!(log.iter().all(|(_, _, thread)| *thread != game_thread), "generation must not run on the game thread");

    let is = |wanted: Stage| move |s: &Stage| *s == wanted;
    // `STRUCTURE_STARTS` radius 8 for references, biomes, terrain and features
    for dz in -8..=8 {
        for dx in -8..=8 {
            let c = ChunkPos::new(target.x + dx, target.z + dz);
            let starts = position(&log, is(Stage::Starts), c);
            if dx.abs() <= 1 && dz.abs() <= 1 {
                // `STRUCTURE_REFERENCES` / `FEATURES` / `SPAWN` of a chunk: every status of the 3x3 is done first
                assert!(starts < position(&log, |s| matches!(s, Stage::References { .. }), target), "{c}");
                assert!(position(&log, is(Stage::Terrain), c) < position(&log, |s| matches!(s, Stage::Features { .. }), target), "terrain {c}");
                assert!(position(&log, is(Stage::Biomes), c) < position(&log, is(Stage::Spawn), target), "biomes {c}");
            }
            assert!(starts < position(&log, |s| matches!(s, Stage::Features { .. }), target), "{c}");
        }
    }
    // per chunk: starts < references < biomes < terrain < features < spawn
    for (stage, pos, _) in &log {
        if *stage == Stage::Spawn {
            let order = [
                position(&log, is(Stage::Starts), *pos),
                position(&log, |s| matches!(s, Stage::References { .. }), *pos),
                position(&log, is(Stage::Biomes), *pos),
                position(&log, is(Stage::Terrain), *pos),
                position(&log, |s| matches!(s, Stage::Features { .. }), *pos),
                position(&log, is(Stage::Spawn), *pos),
            ];
            assert!(order.windows(2).all(|w| w[0] < w[1]), "{pos}: {order:?}");
        }
    }
    // the steps' regions: both read the structure starts up to 8 chunks around (`ChunkStep.directDependencies`); features write 1 chunk around
    for (stage, _, _) in &log {
        match stage {
            Stage::References { read, write } => assert_eq!((*read, *write), (8, -1)),
            Stage::Features { read, write } => assert_eq!((*read, *write), (8, 1)),
            _ => {}
        }
    }
}

#[test]
fn an_exception_in_a_generation_step_crashes_the_server_with_vanilla_s_report() {
    let dir = std::env::temp_dir().join(format!("mc-pipeline-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Arc::default();
    let target = ChunkPos::new(-4, 6);
    let mut level = open(&dir, &log, Some(target));
    let payload = catch_unwind(AssertUnwindSafe(|| level.proto_chunk_blocking(target.x, target.z, ChunkStatus::Features, true).is_some())).expect_err("the step crashes");
    let report = payload.downcast::<ReportedException>().expect("a ReportedException").0;
    assert_eq!(report.title(), "Exception generating new chunk");
    assert!(report.exception().contains("boom"), "{}", report.exception());
    let details = report.get_details();
    assert!(details.contains("-- Chunk to be generated --"), "{details}");
    assert!(details.contains("Status being generated: minecraft:features"), "{details}");
    assert!(details.contains("Location: -4,6"), "{details}");
    assert!(details.contains(&format!("Position hash: {}", target.pack())), "{details}");
    let _ = std::fs::remove_dir_all(&dir);
}
