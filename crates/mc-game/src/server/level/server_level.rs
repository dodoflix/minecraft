//! `ServerLevel`: the structure of a dimension's level and its chunk handling (construction, chunk source, applying the
//! chunk map's events, blocking chunk access, saving and closing). Other slices add fields (entity manager, tick state)
//! and their own `impl ServerLevel` blocks in their modules.

use super::chunk_map::{ChunkMapEvent, ChunkMapParams};
use super::server_chunk_cache::{ChunkFuture, ServerChunkCache};
use super::world_gen::{ChunkGeneratorBackend, WorldGenContext, WorldGenRegion};
use super::{FullChunkStatus, Ticket, TicketType};
use crate::server::shared::ServerShared;
use crate::world::block_entity::BlockEntity;
use crate::world::entity::EntityId;
use crate::world::level::chunk::LevelChunk;
use crate::world::level::entity::{PersistentEntitySectionManager, Visibility};
use crate::world::level::ticket_storage::TicketStorage;
use crate::world::level::{BlockGetter, DimensionKey, LevelHeight, LevelReader, LightLayer};
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_data::block::{Block, BlockState};
use mc_data::fluid::FluidState;
use mc_world::chunk::ProtoChunk;
use crate::world::level::spawner::natural_spawner;
use mc_world::heightmap::HeightmapType;
use mc_world::palette::PalettedContainerFactory;
use mc_world::status::ChunkStatus;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Everything `ServerLevel` needs from the server when it is created (vanilla reads these from `MinecraftServer`,
/// `LevelStem` and the storage access).
pub struct ServerLevelParams {
    pub shared: Arc<ServerShared>,
    pub dimension: DimensionKey,
    pub level_id: String,
    /// `levelStorage.getDimensionPath(dimension)`.
    pub dimension_dir: PathBuf,
    pub level_height: LevelHeight,
    pub sea_level: i32,
    pub has_sky_light: bool,
    pub factory: Arc<PalettedContainerFactory>,
    pub generator: Arc<dyn ChunkGeneratorBackend>,
    pub generate_structures: bool,
    /// The structure/feature/mob stages of this level's generation (`createStructures`, `applyBiomeDecoration`, ...).
    pub hooks: Arc<dyn super::world_gen::WorldGenHooks>,
    pub seed: i64,
    pub is_known_structure: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub sync_writes: bool,
    pub is_debug: bool,
    /// `levelData` (`PrimaryLevelData` for the overworld, `DerivedLevelData` for the other dimensions).
    pub level_data: Box<dyn super::level_data::ServerLevelData>,
}

pub struct ServerLevel {
    pub dimension: DimensionKey,
    /// `entityManager` (+ `entityTickList`): owns every entity of the level (see `server_level_entities.rs`).
    pub entity_manager: PersistentEntitySectionManager,
    /// `players`: ids of the players in this level, in join order.
    pub players: Vec<EntityId>,
    /// `chunkMap.entityMap`: which players see which entity (see `entity_tracker.rs`).
    pub entity_tracker: super::EntityTracker,
    /// Handles to the server-global state (scoreboard, player roster), installed by the server (see `level_server_state.rs`).
    pub server_state: super::LevelServerState,
    /// Snapshots of the players that are currently taken out of the entity storage (see `tracker_players`).
    pub(super) taken_out_players: std::collections::HashMap<EntityId, super::TrackerPlayer>,
    /// `registryAccess()`: the server's registries (damage types, enchantments, tags, ...). Empty until the level
    /// factory installs them (`detached` levels stay empty).
    pub registries: crate::registry::RegistryAccess,

    /// `chunkSource`.
    pub chunk_source: ServerChunkCache,
    pub shared: Arc<ServerShared>,
    /// `serverLevelData`.
    pub level_data: Box<dyn super::level_data::ServerLevelData>,
    /// `noSave`.
    pub no_save: bool,
    /// `dimensionType().hasCeiling()` (set by the level factory).
    pub has_ceiling: bool,
    pub is_debug: bool,
    sea_level: i32,
    level_height: LevelHeight,
    pub factory: Arc<PalettedContainerFactory>,
    /// `Level.skyDarken` (maintained by the tick: `updateSkyBrightness`).
    pub sky_darken: i32,
    /// `Level.getGameTime()` mirror for saving chunks (set by the tick).
    game_time: i64,
    /// Mutation/ticking state (neighbor updater, scheduled ticks, block events, ...): see `level_tick_state.rs`.
    pub tick_state: super::level_tick_state::LevelTickState,
    /// The level's generation environment (`getChunkSource().getGenerator()`, registries, template manager); `None` for
    /// levels without a generator ([`ServerLevel::detached`]).
    pub gen_env: Option<Arc<crate::world::level::levelgen::GenEnv>>,
    /// The worldgen data the generation registries were decoded from (inline `/place feature` definitions decode against it).
    pub worldgen_data: Option<Arc<mc_worldgen::data::WorldgenRegistries>>,
    /// Server clock inputs of `getCurrentDifficultyAt` while a command places worldgen content
    /// (see [`server_level_worldgen::with_worldgen_level`](super::server_level_worldgen::with_worldgen_level)).
    pub worldgen_clock: Option<super::server_level_worldgen::WorldGenClock>,
    // ==== [Damage] additions (ServerLevel field) ====
    /// Operations on entities that were taken out (running) when they were requested: run when put back
    /// (`ServerLevel::with_entity_or_defer`).
    pub deferred_level_ops: std::collections::HashMap<EntityId, Vec<DeferredLevelOp>>,
    // ==== [PathfinderCore] additions (ServerLevel field) ====
    /// `pathTypesByPosCache` (`getPathTypeCache()`), invalidated by `send_block_updated`.
    pub path_type_cache: parking_lot::Mutex<crate::world::level::pathfinder::PathTypeCache>,
    // ==== [SpawnFramework] additions (ServerLevel field) ====
    /// Natural spawning state: spawn state of the tick, custom spawners, dimension attribute base (see `world::level::spawner`).
    pub spawning: crate::world::level::spawner::LevelSpawnData,
    /// The level's generation environment (`getChunkSource().getGenerator()`, registries, template manager); `None` for
    /// levels without a generator ([`ServerLevel::detached`]).
    pub gen_env: Option<Arc<crate::world::level::levelgen::GenEnv>>,
    /// Server clock inputs of `getCurrentDifficultyAt` while a command places worldgen content
    /// (see [`server_level_worldgen::with_worldgen_level`](super::server_level_worldgen::with_worldgen_level)).
    pub worldgen_clock: Option<super::server_level_worldgen::WorldGenClock>,
}

/// A change to an entity that needs the level (kill credit, stats) and runs once the entity is back in storage.
pub type DeferredLevelOp = Box<dyn FnOnce(&mut ServerLevel, &mut dyn crate::world::entity::Entity)>;

impl ServerLevel {
    /// `new ServerLevel(...)` for the chunk side: creates the chunk source over the dimension folder.
    pub fn new(params: ServerLevelParams) -> Self {
        let region_dir = params.dimension_dir.join("region");
        let data_dir = params.dimension_dir.join("data");
        let spawning = crate::world::level::spawner::LevelSpawnData::default();
        let gen_context = Arc::new(WorldGenContext {
            dimension: params.dimension.clone(),
            level_height: params.level_height,
            factory: params.factory.clone(),
            generator: params.generator,
            hooks: params.hooks,
            generate_structures: params.generate_structures,
            seed: params.seed,
            live: spawning.gen_live.clone(),
        });
        let chunk_params = ChunkMapParams {
            dimension: params.dimension.clone(),
            level_id: params.level_id,
            region_dir,
            level_height: params.level_height,
            has_sky_light: params.has_sky_light,
            factory: params.factory.clone(),
            gen_context,
            is_known_structure: params.is_known_structure,
            sync_writes: params.sync_writes,
            view_distance: params.view_distance,
            tickets: TicketStorage::new(),
        };
        let mut chunk_source = ServerChunkCache::new(data_dir, chunk_params, params.simulation_distance);
        let tick_state = super::level_tick_state::LevelTickState::default();
        chunk_source.chunk_map.tick_containers = Some(tick_state.ticks.clone());
        Self {
            dimension: params.dimension,
            entity_manager: PersistentEntitySectionManager::new(None),
            players: Vec::new(),
            entity_tracker: Default::default(),
            server_state: Default::default(),
            taken_out_players: Default::default(),
            registries: Default::default(),
            chunk_source,
            shared: params.shared,
            level_data: params.level_data,
            no_save: false,
            has_ceiling: false,
            is_debug: params.is_debug,
            sea_level: params.sea_level,
            level_height: params.level_height,
            factory: params.factory,
            sky_darken: 0,
            game_time: 0,
            tick_state,
            gen_env: None,
            worldgen_data: None,
            worldgen_clock: None,
            // ==== [Damage] additions (ServerLevel::new initializer) ====
            deferred_level_ops: Default::default(),
            // ==== [PathfinderCore] additions (ServerLevel::new initializer) ====
            path_type_cache: Default::default(),
            // ==== [SpawnFramework] additions (ServerLevel::new initializer) ====
            spawning,
            gen_env: None,
            worldgen_clock: None,
        }
    }

    /// A level over a fresh empty temporary directory with a void generator: for tests of code that needs a
    /// `ServerLevel` but no real world.
    pub fn detached(dimension: DimensionKey) -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("mc-detached-level-{}-{}", std::process::id(), COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        let (shared, _rx) = ServerShared::new();
        let biomes = mc_world::palette::BiomeRegistry::new(vec!["minecraft:plains".to_string()]);
        Self::new(ServerLevelParams {
            shared,
            dimension,
            level_id: "detached".to_string(),
            dimension_dir: dir,
            level_height: LevelHeight { min_y: -64, height: 384 },
            sea_level: 63,
            has_sky_light: true,
            factory: Arc::new(PalettedContainerFactory::create(biomes).expect("plains registered")),
            generator: Arc::new(VoidGenerator),
            generate_structures: false,
            hooks: Arc::new(super::world_gen::NoWorldGenHooks),
            seed: 0,
            is_known_structure: Arc::new(|_| false),
            view_distance: 10,
            simulation_distance: 10,
            sync_writes: false,
            is_debug: false,
            level_data: Box::new(super::level_data::PrimaryServerLevelData(super::level_data::default_world_data())),
        })
    }

    /// `getPathTypeCache()`.
    pub fn path_type_cache(&self) -> &parking_lot::Mutex<crate::world::level::pathfinder::PathTypeCache> {
        &self.path_type_cache
    }

    /// `recipeAccess()`: the server's `RecipeManager`.
    pub fn recipe_access(&self) -> crate::world::item::crafting::RecipeManager {
        self.shared.recipe_manager()
    }

    pub fn chunk_source(&self) -> &ServerChunkCache {
        &self.chunk_source
    }

    pub fn chunk_source_mut(&mut self) -> &mut ServerChunkCache {
        &mut self.chunk_source
    }

    /// `getGameTime()`.
    pub fn game_time(&self) -> i64 {
        self.level_data.get_game_time()
    }

    pub fn set_game_time(&mut self, game_time: i64) {
        self.level_data.set_game_time(game_time);
        self.game_time = game_time;
        self.chunk_source.chunk_map.game_time = game_time;
    }

    // ------------------------------------------------------------------ chunk events

    /// Apply the chunk map's pending events (and everything they cascade into).
    pub fn apply_chunk_events(&mut self) {
        loop {
            let events = self.chunk_source.take_events();
            if events.is_empty() {
                break;
            }
            for event in events {
                self.apply_chunk_event(event);
            }
        }
    }

    fn apply_chunk_event(&mut self, event: ChunkMapEvent) {
        match event {
            ChunkMapEvent::FullStep(pos) => {
                use super::chunk_holder::ChunkSlot;
                let Some(slot) = self.chunk_source.chunk_map.take_chunk_for_full(pos) else { return };
                let chunk = match slot {
                    ChunkSlot::Proto(proto) => {
                        // `postLoadProtoChunk`: legacy entity NBT of the proto chunk is loaded with the entity slice.
                        Box::new(LevelChunk::from_proto(*proto, &self.factory))
                    }
                    ChunkSlot::Level(chunk) => chunk,
                    other => {
                        // The chunk is lent to the worldgen thread (a neighbour's step reads it): retry after the batch.
                        if let Some(h) = self.chunk_source.chunk_map.holders.holder_mut(pos.pack()) {
                            h.slot = other;
                        }
                        self.chunk_source.chunk_map.requeue_full_step(pos);
                        return;
                    }
                };
                let mut chunk = chunk;
                chunk.promote_pending_block_entities();
                chunk.data.set_loaded(true);
                self.chunk_source.chunk_map.complete_full_step(pos, chunk);
                self.register_all_block_entities_after_level_load(pos);
                self.register_tick_container_in_level(pos);
            }
            ChunkMapEvent::StartTicking(pos) => {
                // `postProcessGeneration(level)` and `startTickingChunk` belong to the LevelChunk/tick slice; the chunk
                // map only needs to know the chunk is now ticking.
                self.post_process_generation(pos);
                self.start_ticking_chunk(pos);
                self.chunk_source.chunk_map.complete_ticking(pos);
                self.chunk_source.after_chunk_ready_to_send();
            }
            ChunkMapEvent::FullStatusChanged(pos, status) => {
                self.entity_manager.update_chunk_status(pos, Visibility::from_full_chunk_status(status));
                self.process_entity_events();
            }
            ChunkMapEvent::Unload(chunk) => self.unload(chunk),
            ChunkMapEvent::LegacyEntities(_pos, tags) => {
                let entities = crate::world::entity::factory::load_entities_recursive(&tags, self, mc_data::enums::EntitySpawnReason::Load);
                self.entity_manager.add_legacy_chunk_entities(entities);
                self.process_entity_events();
            }
        }
    }

    /// `ServerLevel.unload(chunk)`.
    fn unload(&mut self, mut chunk: Box<LevelChunk>) {
        // `clearAllBlockEntities` + `unregisterTickContainerFromLevel`
        self.clear_all_block_entities(&mut chunk);
        ServerLevel::unregister_tick_container_from_level(&mut chunk, &self.tick_state.ticks);
        drop(chunk);
    }

    // ------------------------------------------------------------------ polling / blocking access

    /// `ServerChunkCache.pollTask()`: one unit of chunk work, with its events applied.
    pub fn poll_chunk_task(&mut self) -> bool {
        let did = self.chunk_source.poll_task();
        self.apply_chunk_events();
        did
    }

    /// `runDistanceManagerUpdates()` with events applied.
    pub fn run_distance_manager_updates(&mut self) -> bool {
        let updated = self.chunk_source.run_distance_manager_updates();
        self.apply_chunk_events();
        updated
    }

    /// `managedBlock(until)` on the chunk source: run chunk work until `until` holds (worker results arrive on their
    /// own threads, so idle iterations sleep briefly).
    pub fn block_on_chunks(&mut self, until: impl Fn(&ServerChunkCache) -> bool) {
        let start = std::time::Instant::now();
        let mut warned = false;
        while !until(&self.chunk_source) {
            if !self.poll_chunk_task() {
                std::thread::sleep(Duration::from_micros(200));
            }
            if !warned && start.elapsed() > Duration::from_secs(30) {
                warned = true;
                let map = &self.chunk_source.chunk_map;
                log::warn!(
                    "Chunk source has been blocked for 30s: has_work={} main_tasks={} tasks={} worldgen_busy={} loading={} unload_queue={}",
                    map.has_work(),
                    map.main_tasks.len(),
                    map.pending_task_count(),
                    map.worldgen_busy(),
                    map.loading_count(),
                    map.unload_queue_len()
                );
            }
        }
    }

    /// `getChunk(x, z, status, loadOrGenerate)` for `FULL`: loads/generates the chunk and waits for it.
    pub fn chunk_blocking_mut(&mut self, x: i32, z: i32) -> &mut LevelChunk {
        let future = self.chunk_source.get_chunk_future_main_thread(x, z, ChunkStatus::Full, true);
        self.apply_chunk_events();
        self.block_on_chunks(|cs| cs.future_result(future).is_some());
        if self.chunk_source.future_result(future) != Some(true) {
            panic!("Chunk not there when requested: {future:?}");
        }
        self.chunk_source.get_chunk_now_mut(x, z).expect("full chunk present")
    }

    /// `getChunk(x, z, targetStatus, loadOrGenerate)` for statuses below `FULL`: waits and returns the proto chunk.
    pub fn proto_chunk_blocking(&mut self, x: i32, z: i32, status: ChunkStatus, load_or_generate: bool) -> Option<&ProtoChunk> {
        assert!(status != ChunkStatus::Full, "use chunk_blocking_mut for FULL");
        let future = self.chunk_source.get_chunk_future_main_thread(x, z, status, load_or_generate);
        self.apply_chunk_events();
        if future == ChunkFuture::Unloaded {
            return None;
        }
        self.block_on_chunks(|cs| cs.future_result(future).is_some());
        if self.chunk_source.future_result(future) != Some(true) {
            return None;
        }
        self.chunk_source.proto_chunk(ChunkPos::pack_xz(x, z))
    }

    /// `getChunkNow(x, z)`.
    pub fn chunk_now(&self, x: i32, z: i32) -> Option<&LevelChunk> {
        self.chunk_source.get_chunk_now(x, z)
    }

    pub fn chunk_now_mut(&mut self, x: i32, z: i32) -> Option<&mut LevelChunk> {
        self.chunk_source.get_chunk_now_mut(x, z)
    }

    /// `ServerChunkCache.blockChanged(pos)`.
    pub fn block_changed(&mut self, pos: BlockPos) {
        self.chunk_source.block_changed(pos);
    }

    /// `ChunkHolder.getFullStatus()` of the chunk, `Inaccessible` if there is no holder.
    pub fn full_chunk_status(&self, cx: i32, cz: i32) -> FullChunkStatus {
        self.chunk_source
            .chunk_map
            .get_visible_chunk_if_present(ChunkPos::pack_xz(cx, cz))
            .map_or(FullChunkStatus::Inaccessible, |h| h.get_full_status())
    }

    /// The light engine (`getLightEngine()`).
    pub fn light_engine(&self) -> &mc_light::ThreadedLevelLightEngine {
        &self.chunk_source.chunk_map.light
    }

    pub fn light_engine_mut(&mut self) -> &mut mc_light::ThreadedLevelLightEngine {
        &mut self.chunk_source.chunk_map.light
    }

    /// `addTicket`/`addRegionTicket` convenience used by commands, portals, pearls.
    pub fn add_ticket(&mut self, ticket: Ticket, pos: ChunkPos) {
        self.chunk_source.add_ticket(ticket, pos);
    }

    pub fn add_ticket_with_radius(&mut self, ty: TicketType, pos: ChunkPos, radius: i32) {
        self.chunk_source.add_ticket_with_radius(ty, pos, radius);
    }

    // ------------------------------------------------------------------ ticking

    /// `ServerChunkCache.tick(haveTime, tickChunks = false)` (levels that only run chunk loading).
    pub fn tick_chunk_source(&mut self, have_time: &dyn Fn() -> bool, tick_chunks: bool, runs_normally: bool) {
        assert!(!tick_chunks, "tickChunks needs the tick environment: use tick_chunk_source_with_env");
        self.tick_chunk_source_inner(None, have_time, runs_normally);
    }

    /// `ServerChunkCache.tick(haveTime, true)`.
    pub fn tick_chunk_source_with_env(&mut self, env: &dyn super::level_tick::LevelTickEnv, have_time: &dyn Fn() -> bool, runs_normally: bool) {
        self.tick_chunk_source_inner(Some(env), have_time, runs_normally);
    }

    fn tick_chunk_source_inner(&mut self, env: Option<&dyn super::level_tick::LevelTickEnv>, have_time: &dyn Fn() -> bool, runs_normally: bool) {
        self.chunk_source.tick_pre(runs_normally, env.is_some());
        self.apply_chunk_events();
        if let Some(env) = env {
            self.tick_chunks(runs_normally, env);
            self.tick_chunk_map();
        }
        self.chunk_source.tick_unload(have_time);
        self.apply_chunk_events();
    }

    /// `ChunkMap.tick()`'s player tracking refresh (the entity tracking half is `entity_tracker`).
    fn tick_chunk_map(&mut self) {
        let ids: Vec<EntityId> = self.chunk_source.chunk_map.player_map.get_all_players().collect();
        for id in ids {
            self.chunk_source.chunk_map.update_chunk_tracking(id);
        }
        self.tick_entity_tracker();
    }

    /// `ServerChunkCache.tickChunks()`.
    fn tick_chunks(&mut self, runs_normally: bool, env: &dyn super::level_tick::LevelTickEnv) {
        use mc_core::random::RandomSource;
        if self.is_debug {
            return;
        }
        if runs_normally {
            // naturalSpawnCount + NaturalSpawner.createState + spawning categories
            let chunk_count = self.chunk_source.chunk_map.distance_manager.get_natural_spawn_chunk_count();
            let do_mob_spawning = self.spawn_mobs();
            let tick_speed = self.random_tick_speed();
            let spawn_enemies = self.chunk_source.spawn_enemies();
            let spawn_persistent = self.game_time() % 400 == 0;
            // `NaturalSpawner.createState` (always) + `getFilteredSpawningCategories` (only while `spawnMobs`)
            let state = natural_spawner::create_state(chunk_count as i32, self);
            let categories = if do_mob_spawning { natural_spawner::get_filtered_spawning_categories(&state, spawn_enemies, spawn_persistent) } else { Vec::new() };
            self.spawning.set_spawn_state(state, categories);

            // collectSpawningChunks, Util.shuffle(spawningChunks, level.getRandom())
            let mut spawning: Vec<ChunkPos> = self.chunk_source.chunk_map.collect_spawning_chunks().into_iter().map(ChunkPos::unpack).collect();
            for i in (2..=spawning.len()).rev() {
                let swap_to = self.tick_state.random.next_int_bounded(i as i32) as usize;
                spawning.swap(i - 1, swap_to);
            }
            for pos in spawning {
                self.tick_spawning_chunk(pos, env);
            }

            // tickTickingChunks
            let ticking: Vec<ChunkPos> = self.chunk_source.chunk_map.block_ticking_chunks().into_iter().map(ChunkPos::unpack).collect();
            for pos in ticking {
                self.tick_chunk(pos, tick_speed);
            }
            if do_mob_spawning {
                self.tick_custom_spawners(spawn_enemies);
            }
        }
        self.chunk_source.broadcast_changed_chunks();
    }

    /// `ServerChunkCache.tickSpawningChunk(chunk, spawningCategories, spawnCookie)`.
    fn tick_spawning_chunk(&mut self, pos: ChunkPos, env: &dyn super::level_tick::LevelTickEnv) {
        let Some(chunk) = self.chunk_now_mut(pos.x, pos.z) else { return };
        chunk.data.access.increment_inhabited_time();
        if self.chunk_source.chunk_map.distance_manager.in_entity_ticking_range(pos.pack()) {
            self.tick_thunder(pos, env);
        }
        if self.spawning.has_spawning_categories() && self.can_spawn_entities_in_chunk(pos) {
            natural_spawner::spawn_for_chunk(self, pos);
        }
    }

    /// `canSpawnEntitiesInChunk(pos)`: `entityManager.canPositionTick(pos) && worldBorder.isWithinBounds(pos)`.
    pub fn can_spawn_entities_in_chunk(&self, pos: ChunkPos) -> bool {
        self.entity_manager.is_ticking(pos) && self.is_within_world_border_chunk(pos)
    }

    // ------------------------------------------------------------------ saving / closing

    /// `ServerChunkCache.save(flushStorage)`.
    pub fn save_chunks(&mut self, flush_storage: bool) {
        self.run_distance_manager_updates();
        if flush_storage {
            let keys = self.chunk_source.chunk_map.collect_chunks_to_save();
            loop {
                let mut did_work = false;
                for key in &keys {
                    let key = *key;
                    self.block_on_chunks(|cs| cs.chunk_map.is_ready_for_saving(key));
                    if self.chunk_source.chunk_map.save_full_chunk_for_flush(key) {
                        did_work = true;
                    }
                }
                if !did_work {
                    break;
                }
            }
            self.chunk_source.chunk_map.process_unloads(&|| true);
            self.apply_chunk_events();
            self.chunk_source.chunk_map.synchronize(true);
        } else {
            self.chunk_source.chunk_map.save_all_chunks_periodic();
        }
        self.chunk_source.sync_tickets_to_data_storage();
    }

    /// `getDataStorage()`: the dimension's saved data storage (`<dimension>/data`).
    pub fn get_data_storage(&mut self) -> &mut crate::world::level::saveddata::SavedDataManager {
        &mut self.chunk_source.data_storage
    }

    /// `ServerChunkCache.close()` / `ServerLevel.close()`.
    pub fn close(&mut self) {
        self.save_chunks(true);
        self.chunk_source.close_data_storage();
        self.chunk_source.chunk_map.close();
    }
}

/// A generator that leaves chunks empty (void): used by [`ServerLevel::detached`].
struct VoidGenerator;

impl ChunkGeneratorBackend for VoidGenerator {
    fn type_name_for_data_fixer(&self) -> Option<mc_core::ident::Identifier> {
        None
    }
    fn create_biomes(&self, _chunk: &mut ProtoChunk, _region: &WorldGenRegion) {}
    fn build_terrain(&self, _chunk: &mut ProtoChunk, _region: &WorldGenRegion) {}
    fn get_base_height(&self, _x: i32, _z: i32, _ty: HeightmapType, height: LevelHeight) -> i32 {
        height.min_y
    }
}

// ---------------------------------------------------------------------- level access traits

impl BlockGetter for ServerLevel {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        if self.is_outside_build_height(pos.y()) {
            return Block::VOID_AIR.default_state();
        }
        match self.chunk_now(pos.x() >> 4, pos.z() >> 4) {
            Some(chunk) => chunk.data.get_block_state(pos),
            // Vanilla loads the chunk here; `&self` cannot, callers that need loading use `chunk_blocking_mut`.
            None => Block::AIR.default_state(),
        }
    }

    fn get_fluid_state(&self, pos: BlockPos) -> FluidState {
        if self.is_outside_build_height(pos.y()) {
            return FluidState::EMPTY;
        }
        match self.chunk_now(pos.x() >> 4, pos.z() >> 4) {
            Some(chunk) => chunk.data.get_fluid_state(pos.x(), pos.y(), pos.z()),
            None => FluidState::EMPTY,
        }
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<&dyn BlockEntity> {
        self.chunk_now(pos.x() >> 4, pos.z() >> 4)?.block_entities.get(&pos).map(|b| &**b)
    }

    fn level_height(&self) -> LevelHeight {
        self.level_height
    }
}

impl LevelReader for ServerLevel {
    fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunk_source.has_chunk(chunk_x, chunk_z)
    }

    fn get_height_at(&self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        if x >= -30_000_000 && z >= -30_000_000 && x < 30_000_000 && z < 30_000_000 {
            if self.has_chunk(x >> 4, z >> 4) {
                if let Some(chunk) = self.chunk_now(x >> 4, z >> 4) {
                    if let Some(h) = chunk.data.access.heightmaps.get(&ty) {
                        return h.get_first_available(x & 15, z & 15);
                    }
                    // `ChunkAccess.getHeight` primes a heightmap the chunk does not maintain (the `_WG` types)
                    let mut primed = mc_world::heightmap::HeightmapMap::new();
                    mc_world::heightmap::Heightmap::prime_heightmaps(&chunk.data.access.view(true), &mut primed, &[ty]);
                    return primed[&ty].get_first_available(x & 15, z & 15);
                }
            }
            self.get_min_y()
        } else {
            self.sea_level + 1
        }
    }

    fn enabled_features(&self) -> mc_world::level_storage::FeatureFlagSet {
        self.tick_state.enabled_features
    }

    fn get_sea_level(&self) -> i32 {
        self.sea_level
    }

    fn get_brightness(&self, layer: LightLayer, pos: BlockPos) -> i32 {
        let layer = if layer == LightLayer::Sky { mc_light::LightLayer::Sky } else { mc_light::LightLayer::Block };
        self.light_engine().engine().get_light_value(layer, pos)
    }

    fn get_sky_darken(&self) -> i32 {
        self.sky_darken
    }

    fn get_raw_brightness(&self, pos: BlockPos, sky_darkening: i32) -> i32 {
        self.light_engine().engine().get_raw_brightness(pos, sky_darkening)
    }

    fn can_see_sky(&self, pos: BlockPos) -> bool {
        self.get_brightness(LightLayer::Sky, pos) >= 15
    }

    fn ambient_light(&self) -> f32 {
        self.tick_state.ambient_light
    }
}

/// `SectionPos` helper for callers that hold a block position.
pub fn section_of_block(pos: BlockPos) -> SectionPos {
    SectionPos::of_block_pos(&pos)
}
