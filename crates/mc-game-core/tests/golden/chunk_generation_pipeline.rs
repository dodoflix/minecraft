//! The chunk status pipeline's world generation stages: the hooks (`createStructures`, `createReferences`,
//! `applyBiomeDecoration`, `spawnOriginalMobs`) run on the worldgen thread in vanilla's `ChunkPyramid` order with the
//! step's region (read/write radius), and an exception in a step crashes the server with vanilla's report
//! (`ChunkMap.applyStep` / `GenerationChunkHolder.applyStep` -> `relayDelayCrash`).

use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_core::pos::ChunkPos;
use mc_data::block::{Block, BlockState};
use mc_game::crash_report::ReportedException;
use mc_game::server::level::world_gen::{
    ChunkGeneratorBackend, WorldGenContext, WorldGenHooks, WorldGenRegion,
};
use mc_game::server::level::{ServerLevel, ServerLevelParams};
use mc_game::server::shared::ServerShared;
use mc_world::below_zero_retrogen::BelowZeroRetrogen;
use mc_world::blending_data::{BlendingData, BlendingDataPacked};
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::palette::{BiomeRegistry, PalettedContainerFactory};
use mc_world::status::ChunkStatus;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stage {
    Starts,
    References {
        read: i32,
        write: i32,
    },
    Biomes,
    Terrain,
    /// `Blender.of(region)` of the biome (`terrain` false) or terrain step: whether it is `Blender.EMPTY`.
    Blender {
        terrain: bool,
        empty: bool,
    },
    Features {
        read: i32,
        write: i32,
    },
    Spawn,
}

type Log = Arc<Mutex<Vec<(Stage, ChunkPos, std::thread::ThreadId)>>>;

/// Counts the terrain steps that run at the same moment.
#[derive(Default)]
struct Overlap {
    active: AtomicUsize,
    max: AtomicUsize,
}

struct Running<'a>(&'a Overlap);

impl Overlap {
    fn enter(&self) -> Running<'_> {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(50));
        Running(self)
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `legacy`: every chunk is an old-version chunk (below zero retrogen + blending data) with old bedrock at y 0..=4, new
/// bedrock in the below-zero column (x 3, z 2) and water at the bottom of the old generation area.
struct Generator {
    log: Log,
    legacy: bool,
    overlap: Option<Arc<Overlap>>,
    /// Nonzero: the biome and terrain steps of a chunk take a time that depends on this value and the chunk.
    jitter: u64,
}

impl Generator {
    /// A slow or fast worker thread: up to 2.5 ms per step, a different one per chunk and `jitter`.
    fn work(&self, pos: ChunkPos) {
        if self.jitter == 0 {
            return;
        }
        let mut z = self.jitter ^ (u64::from(pos.x as u32) << 32 | u64::from(pos.z as u32));
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        std::thread::sleep(Duration::from_micros((z ^ (z >> 31)) % 2500));
    }
}

impl ChunkGeneratorBackend for Generator {
    fn type_name_for_data_fixer(&self) -> Option<Identifier> {
        Identifier::parse("minecraft:noise").ok()
    }
    fn create_biomes(&self, chunk: &mut ProtoChunk, region: &WorldGenRegion) {
        self.work(chunk.pos());
        let mut log = self.log.lock().unwrap();
        log.push((Stage::Biomes, chunk.pos(), std::thread::current().id()));
        log.push((
            Stage::Blender {
                terrain: false,
                empty: region.blender.is_empty(),
            },
            chunk.pos(),
            std::thread::current().id(),
        ));
        drop(log);
        if self.legacy {
            let mut retrogen = mc_nbt::CompoundTag::new();
            retrogen.put_string("target_status", "minecraft:features");
            // the column (x 3, z 2) lost its bedrock
            retrogen.put_long_array("missing_bedrock", vec![1 << 35]);
            chunk.set_below_zero_retrogen(BelowZeroRetrogen::parse(&mc_nbt::Tag::Compound(
                retrogen,
            )));
            let packed = BlendingDataPacked {
                min_section: -4,
                max_section: 8,
                heights: None,
            };
            chunk.access.blending_data = BlendingData::unpack(Some(&packed));
        }
    }
    fn build_terrain(&self, chunk: &mut ProtoChunk, region: &WorldGenRegion) {
        let _running = self.overlap.as_ref().map(|overlap| overlap.enter());
        self.work(chunk.pos());
        let mut log = self.log.lock().unwrap();
        log.push((Stage::Terrain, chunk.pos(), std::thread::current().id()));
        log.push((
            Stage::Blender {
                terrain: true,
                empty: region.blender.is_empty(),
            },
            chunk.pos(),
            std::thread::current().id(),
        ));
        drop(log);
        if self.legacy {
            for y in 0..=4 {
                chunk.set_block_state(BlockPos::new(0, y, 0), Block::BEDROCK.default_state(), None);
            }
            for y in -64..=-60 {
                chunk.set_block_state(BlockPos::new(3, y, 2), Block::BEDROCK.default_state(), None);
            }
            chunk.set_block_state(BlockPos::new(5, -64, 7), Block::WATER.default_state(), None);
        }
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

struct Hooks {
    log: Log,
    /// Panic in `applyBiomeDecoration` of this chunk.
    fail_features_at: Option<ChunkPos>,
    /// A raw section write (an ore vein) of `applyBiomeDecoration` of the chunk.
    section_write: Option<(ChunkPos, BlockPos, BlockState)>,
    /// `createStructures` of this chunk takes 100 ms, so every other task is ready when the batch it belongs to is done.
    slow_starts_at: Vec<ChunkPos>,
}

impl Hooks {
    fn record(&self, stage: Stage, pos: ChunkPos) {
        self.log
            .lock()
            .unwrap()
            .push((stage, pos, std::thread::current().id()));
    }
}

impl WorldGenHooks for Hooks {
    fn create_structures(&self, _ctx: &WorldGenContext, chunk: &mut ProtoChunk) {
        if self.slow_starts_at.contains(&chunk.pos()) {
            std::thread::sleep(Duration::from_millis(100));
        }
        self.record(Stage::Starts, chunk.pos());
    }
    fn create_references(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.record(
            Stage::References {
                read: region.read_radius,
                write: region.write_radius,
            },
            region.center,
        );
    }
    fn apply_biome_decoration(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.record(
            Stage::Features {
                read: region.read_radius,
                write: region.write_radius,
            },
            region.center,
        );
        if self.fail_features_at == Some(region.center) {
            panic!("boom");
        }
        if let Some((center, pos, state)) = self.section_write
            && center == region.center
        {
            region.section_writes.push((pos, state));
        }
    }
    fn spawn_original_mobs(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.record(Stage::Spawn, region.center);
    }
}

fn open(
    dir: &std::path::Path,
    log: &Log,
    fail_features_at: Option<ChunkPos>,
    legacy: bool,
) -> ServerLevel {
    open_with(dir, log, fail_features_at, legacy, None, None, 0, &[])
}

#[allow(
    clippy::too_many_arguments,
    reason = "one knob per scenario of this file"
)]
fn open_with(
    dir: &std::path::Path,
    log: &Log,
    fail_features_at: Option<ChunkPos>,
    legacy: bool,
    overlap: Option<Arc<Overlap>>,
    section_write: Option<(ChunkPos, BlockPos, BlockState)>,
    jitter: u64,
    slow_starts_at: &[ChunkPos],
) -> ServerLevel {
    let (shared, _rx) = ServerShared::new();
    ServerLevel::new(ServerLevelParams {
        shared,
        dimension: Identifier::parse("minecraft:overworld").unwrap(),
        level_id: "pipeline".into(),
        dimension_dir: dir.to_path_buf(),
        level_height: LevelHeight {
            min_y: -64,
            height: 384,
        },
        sea_level: 63,
        has_sky_light: true,
        factory: Arc::new(
            PalettedContainerFactory::create(BiomeRegistry::new(vec![
                "minecraft:plains".to_string(),
            ]))
            .unwrap(),
        ),
        generator: Arc::new(Generator {
            log: log.clone(),
            legacy,
            overlap,
            jitter,
        }),
        generate_structures: true,
        hooks: Arc::new(Hooks {
            log: log.clone(),
            fail_features_at,
            section_write,
            slow_starts_at: slow_starts_at.to_vec(),
        }),
        seed: 0,
        is_known_structure: Arc::new(|_| false),
        view_distance: 4,
        simulation_distance: 4,
        sync_writes: false,
        is_debug: false,
        level_data: Box::new(mc_game::server::level::level_data::PrimaryServerLevelData(
            mc_game::server::level::level_data::default_world_data(),
        )),
    })
}

fn position(
    log: &[(Stage, ChunkPos, std::thread::ThreadId)],
    stage: impl Fn(&Stage) -> bool,
    pos: ChunkPos,
) -> usize {
    log.iter()
        .position(|(s, p, _)| stage(s) && *p == pos)
        .unwrap_or_else(|| panic!("stage missing for {pos}"))
}

/// `buildTerrain` runs on `Util.backgroundExecutor()`: the terrain of different chunks is generated at the same time,
/// not one chunk after the other like the serial worldgen steps.
#[test]
fn terrain_of_different_chunks_runs_at_the_same_time() {
    if std::thread::available_parallelism().map_or(1, |n| n.get()) < 3 {
        return; // the pool has `cores - 1` threads
    }
    let dir = std::env::temp_dir().join(format!("mc-pipeline-overlap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Arc::default();
    let overlap = Arc::new(Overlap::default());
    let mut level = open_with(&dir, &log, None, false, Some(overlap.clone()), None, 0, &[]);
    assert!(
        level
            .proto_chunk_blocking(0, 0, ChunkStatus::Spawn, true)
            .is_some()
    );
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        overlap.max.load(Ordering::SeqCst) >= 2,
        "terrain steps never overlapped"
    );
}

/// How fast the threads of the biome and terrain steps are decides when their results arrive, not which step the chunk
/// map starts next: the serial steps (feature decoration of neighbours is order dependent) run in the same order however
/// the background steps are timed.
#[test]
fn the_serial_steps_run_in_the_same_order_whatever_the_speed_of_the_workers() {
    let serial_order = |jitter: u64| {
        let dir = std::env::temp_dir().join(format!(
            "mc-pipeline-jitter-{}-{jitter}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let log: Log = Arc::default();
        let mut level = open_with(&dir, &log, None, false, None, None, jitter, &[]);
        for (x, z) in [(0, 0), (1, 0), (2, 1)] {
            assert!(
                level
                    .proto_chunk_blocking(x, z, ChunkStatus::Spawn, true)
                    .is_some()
            );
        }
        level.block_on_chunks(|chunks| {
            let map = &chunks.chunk_map;
            map.pending_task_count() == 0
                && !map.worldgen_busy()
                && map.loading_count() == 0
                && map.main_tasks.is_empty()
        });
        level.close();
        let _ = std::fs::remove_dir_all(&dir);
        let log = log.lock().unwrap();
        log.iter()
            .filter(|(stage, _, _)| {
                !matches!(
                    stage,
                    Stage::Biomes | Stage::Terrain | Stage::Blender { .. }
                )
            })
            .map(|(stage, pos, _)| (stage.clone(), *pos))
            .collect::<Vec<_>>()
    };
    let reference = serial_order(0);
    assert!(!reference.is_empty());
    for jitter in 1..=4 {
        assert_eq!(serial_order(jitter), reference, "jitter {jitter}");
    }
}

/// One `runUntilWait()` slice goes on through every layer whose steps finish inside the call: the structure starts and the
/// references of a task run one after the other on the worker, however many other tasks are ready (the steps' futures are
/// complete when the loop checks them, so the slice is never queued behind another chunk's task in between).
#[test]
fn a_slice_runs_the_starts_and_references_layers_without_another_task_in_between() {
    let dir = std::env::temp_dir().join(format!("mc-pipeline-slice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Arc::default();
    // two chunks too far apart to share a step; the first step of each region holds the worker while the other task is ready
    let (west, east) = (ChunkPos::new(0, 0), ChunkPos::new(60, 0));
    let mut level = open_with(&dir, &log, None, false, None, None, 0, &[west, east]);
    let futures = [west, east].map(|c| {
        level
            .chunk_source
            .get_chunk_future_main_thread(c.x, c.z, ChunkStatus::Spawn, true)
    });
    level.apply_chunk_events();
    level.block_on_chunks(|chunks| futures.iter().all(|f| chunks.future_result(*f).is_some()));
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
    let log = log.lock().unwrap();
    // the region of each task, in the order the worker ran the starts and references steps
    let regions: Vec<bool> = log
        .iter()
        .filter(|(stage, _, _)| matches!(stage, Stage::Starts | Stage::References { .. }))
        .map(|(_, pos, _)| pos.x > 30)
        .collect();
    let switches = regions.windows(2).filter(|w| w[0] != w[1]).count();
    assert_eq!(
        switches, 1,
        "the starts/references steps of the two tasks interleave: {regions:?}"
    );
}

#[test]
fn stages_run_in_chunk_pyramid_order_on_the_worldgen_thread() {
    let dir = std::env::temp_dir().join(format!("mc-pipeline-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Arc::default();
    let mut level = open(&dir, &log, None, false);
    let target = ChunkPos::new(3, -2);
    assert!(
        level
            .proto_chunk_blocking(target.x, target.z, ChunkStatus::Spawn, true)
            .is_some()
    );
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
    let log = log.lock().unwrap().clone();

    let game_thread = std::thread::current().id();
    assert!(
        log.iter().all(|(_, _, thread)| *thread != game_thread),
        "generation must not run on the game thread"
    );

    let is = |wanted: Stage| move |s: &Stage| *s == wanted;
    // `STRUCTURE_STARTS` radius 8 for references, biomes, terrain and features
    for dz in -8..=8 {
        for dx in -8..=8 {
            let c = ChunkPos::new(target.x + dx, target.z + dz);
            let starts = position(&log, is(Stage::Starts), c);
            if dx.abs() <= 1 && dz.abs() <= 1 {
                // `STRUCTURE_REFERENCES` / `FEATURES` / `SPAWN` of a chunk: every status of the 3x3 is done first
                assert!(
                    starts < position(&log, |s| matches!(s, Stage::References { .. }), target),
                    "{c}"
                );
                assert!(
                    position(&log, is(Stage::Terrain), c)
                        < position(&log, |s| matches!(s, Stage::Features { .. }), target),
                    "terrain {c}"
                );
                assert!(
                    position(&log, is(Stage::Biomes), c) < position(&log, is(Stage::Spawn), target),
                    "biomes {c}"
                );
            }
            assert!(
                starts < position(&log, |s| matches!(s, Stage::Features { .. }), target),
                "{c}"
            );
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
    let mut level = open(&dir, &log, Some(target), false);
    let payload = catch_unwind(AssertUnwindSafe(|| {
        level
            .proto_chunk_blocking(target.x, target.z, ChunkStatus::Features, true)
            .is_some()
    }))
    .expect_err("the step crashes");
    let report = payload
        .downcast::<ReportedException>()
        .expect("a ReportedException")
        .0;
    assert_eq!(report.title(), "Exception generating new chunk");
    assert!(
        report.exception().contains("boom"),
        "{}",
        report.exception()
    );
    let details = report.get_details();
    assert!(details.contains("-- Chunk to be generated --"), "{details}");
    assert!(
        details.contains("Status being generated: minecraft:features"),
        "{details}"
    );
    assert!(details.contains("Location: -4,6"), "{details}");
    assert!(
        details.contains(&format!("Position hash: {}", target.pack())),
        "{details}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `ChunkStatusTasks.buildTerrain` fixes the bedrock of below zero retrogen chunks and `generateFeatures` ends with
/// `Blender.generateBorderTicks` (old chunks mark their fluids/leaves at the old area's bounds for post processing).
#[test]
fn old_chunks_get_retrogen_bedrock_and_border_ticks() {
    let dir = std::env::temp_dir().join(format!("mc-pipeline-legacy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let log: Log = Arc::default();
    let mut level = open(&dir, &log, None, true);
    let target = ChunkPos::new(1, 1);
    let chunk = level
        .proto_chunk_blocking(target.x, target.z, ChunkStatus::Terrain, true)
        .expect("terrain");
    let name = |x, y, z| {
        chunk
            .get_block_state(BlockPos::new(target.x * 16 + x, y, target.z * 16 + z))
            .block()
    };
    assert!(
        chunk
            .get_below_zero_retrogen()
            .unwrap()
            .has_bedrock_hole(3, 2),
        "{:?}",
        chunk.get_below_zero_retrogen()
    );
    for y in 0..=4 {
        // `replaceOldBedrock`: old bedrock in y 0..=4 becomes deepslate
        assert_eq!(name(0, y, 0), Block::DEEPSLATE, "y {y}");
    }
    for y in -64..=-1 {
        // `applyBedrockMask`: the below-zero part of a column with a bedrock hole is air
        assert_eq!(name(3, y, 2), Block::AIR, "y {y}");
    }
    assert!(chunk.get_below_zero_retrogen().is_some());
    let chunk = level
        .proto_chunk_blocking(target.x, target.z, ChunkStatus::Features, true)
        .expect("features");
    assert!(
        chunk.get_below_zero_retrogen().is_none(),
        "the retrogen ends at its target status"
    );
    // the water at the old area's lowest y: marked for post processing (section 0, one packed offset)
    assert_eq!(
        chunk.access.post_processing[0].as_ref().map(Vec::len),
        Some(1)
    );
    assert!(
        chunk.access.post_processing[1..]
            .iter()
            .all(Option::is_none)
    );
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `OreFeature` reaches the sections of a full chunk next to the chunk being decorated through
/// `ImposterProtoChunk.getSection` (which ignores `allowWrites`): the vein lands in the real chunk of the level, not only in
/// the snapshot the step read.
#[test]
fn ore_writes_into_a_full_neighbour_land_in_the_real_chunk() {
    let dir = std::env::temp_dir().join(format!("mc-pipeline-ore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // a first run stores the full chunk at the origin
    let mut level = open(&dir, &Arc::default(), None, false);
    level.chunk_blocking_mut(0, 0);
    level.close();
    // the features of the chunk next to it write a vein into it
    let pos = BlockPos::new(5, 90, 5);
    let ore = Block::DIAMOND_ORE.default_state();
    let mut level = open_with(
        &dir,
        &Arc::default(),
        None,
        false,
        None,
        Some((ChunkPos::new(1, 0), pos, ore)),
        0,
        &[],
    );
    assert!(
        level
            .proto_chunk_blocking(1, 0, ChunkStatus::Features, true)
            .is_some()
    );
    let chunk = level.chunk_blocking_mut(0, 0);
    assert_eq!(chunk.data.get_block_state(pos), ore);
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `Blender.of(region)` asks the level's IO worker whether a chunk within 7 chunks was stored before `DataVersion` 4997 or
/// with `blending_data`: the steps of a new chunk next to chunks of an earlier run blend, those in a fresh world do not.
#[test]
fn biome_and_terrain_steps_blend_next_to_stored_old_chunks() {
    let dir = std::env::temp_dir().join(format!("mc-pipeline-blend-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let blender_is_empty = |log: &Log, pos: ChunkPos, terrain: bool| {
        log.lock()
            .unwrap()
            .iter()
            .find_map(|(stage, at, _)| match stage {
                Stage::Blender { terrain: t, empty } if *at == pos && *t == terrain => Some(*empty),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no step for {pos}"))
    };
    // the biome step is 7 chunks from the old chunk, the terrain step next to it
    let (far, near) = (ChunkPos::new(7, 0), ChunkPos::new(1, 0));
    let generate = |log: &Log, legacy: bool| {
        let mut level = open(&dir, log, None, legacy);
        if legacy {
            level.chunk_blocking_mut(0, 0);
        } else {
            for pos in [far, near] {
                level
                    .proto_chunk_blocking(pos.x, pos.z, ChunkStatus::Terrain, true)
                    .expect("terrain");
            }
        }
        level
    };
    // fresh world: nothing is old
    let log: Log = Arc::default();
    generate(&log, false).close();
    assert!(blender_is_empty(&log, far, false));
    assert!(blender_is_empty(&log, near, true));
    let _ = std::fs::remove_dir_all(&dir);
    // a first run stores the full chunk at the origin with blending data
    generate(&Arc::default(), true).close();
    let log: Log = Arc::default();
    let mut level = generate(&log, false);
    assert!(!blender_is_empty(&log, far, false));
    assert!(!blender_is_empty(&log, near, true));
    // `BlendingData.calculateData` ran on the old chunk the steps only read
    let old = level.chunk_blocking_mut(0, 0);
    assert!(
        old.data
            .access
            .blending_data
            .as_ref()
            .unwrap()
            .has_calculated_data
    );
    level.close();
    let _ = std::fs::remove_dir_all(&dir);
}
