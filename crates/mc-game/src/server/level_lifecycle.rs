//! The parts of `MinecraftServer` that touch the levels: `loadLevel`/`createLevels`/`prepareLevels`, one tick of the
//! children (`tickChildren`), saving and the stop sequence.

use super::MinecraftServer;
use super::notifications::NotificationService;
use super::level_load_listener::Stage;
use super::level_tick_env::ServerTickEnv;
use super::shared::get_nanos;
use super::tick_pacing::{self, PREPARE_LEVELS_DEFAULT_DELAY_NANOS};
use super::world_dimensions::overworld_key;
use crate::crash_report::{CrashReport, ReportedException};
use crate::registry::keys;
use crate::registry::typed::dimension_type::DimensionType;
use crate::registry::typed::level_stem::{DimensionTypeSource, LevelStem};
use crate::server::level::chunk_load_counter::ChunkLoadCounter;
use crate::server::level::level_data::{DerivedLevelData, PrimaryServerLevelData, ServerLevelData};
use crate::server::level::worldgen_adapter::NoiseWorldGenerator;
use crate::server::level::{ServerLevel, ServerLevelParams};
use crate::world::level::DimensionKey;
use crate::world::level::gamerules::GameRules;
use mc_core::ident::Identifier;
use mc_world::chunk::LevelHeight;
use mc_world::level_storage::{Difficulty, GameType, RespawnData};
use mc_world::palette::PalettedContainerFactory;
use mc_worldgen::biome::{BiomeManager, BiomeRegistry, biome_source_from_json};
use mc_worldgen::data::WorldgenRegistries;
use mc_worldgen::flat::{FlatLevelGeneratorSettings, FlatLevelSource};
use mc_worldgen::generator::{ChunkGenerator, NoiseBasedChunkGenerator};
use mc_worldgen::noise_settings::NoiseGeneratorSettings;
use mc_worldgen::random_state::RandomState;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

/// Everything shared by the levels of one world (`createLevels`' locals).
struct LevelFactory {
    worldgen: Arc<WorldgenRegistries>,
    biomes: Arc<BiomeRegistry>,
    factory: Arc<PalettedContainerFactory>,
    known_structures: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

/// The chunk generator of a `LevelStem` plus the noise settings its `RandomState` is created from.
fn build_generator(
    generator: &serde_json::Value,
    factory: &LevelFactory,
) -> Result<(Arc<dyn ChunkGenerator>, Arc<NoiseGeneratorSettings>), String> {
    let ty = generator.get("type").and_then(|t| t.as_str()).unwrap_or_default();
    let overworld_settings = || -> Result<Arc<NoiseGeneratorSettings>, String> {
        let id = Identifier::with_default_namespace("overworld").expect("valid id");
        factory.worldgen.noise_settings(&id).map(Arc::new).map_err(|e| e.to_string())
    };
    match ty {
        "minecraft:noise" => {
            let source_json = generator.get("biome_source").ok_or("No key biome_source")?;
            let source = biome_source_from_json(source_json, &factory.worldgen, &factory.biomes).map_err(|e| e.to_string())?;
            let settings = match generator.get("settings") {
                Some(serde_json::Value::String(id)) => {
                    let id = Identifier::try_parse(id).ok_or_else(|| format!("Not a valid resource location: {id}"))?;
                    Arc::new(factory.worldgen.noise_settings(&id).map_err(|e| e.to_string())?)
                }
                Some(inline) => Arc::new(NoiseGeneratorSettings::from_json(inline, &factory.worldgen).map_err(|e| e.to_string())?),
                None => return Err("No key settings".to_owned()),
            };
            let noise = NoiseBasedChunkGenerator::new(source, settings.clone(), factory.worldgen.clone(), factory.biomes.clone()).map_err(|e| e.to_string())?;
            Ok((Arc::new(noise), settings))
        }
        "minecraft:flat" => {
            let settings_json = generator.get("settings").ok_or("No key settings")?;
            let settings = FlatLevelGeneratorSettings::from_json(settings_json, &factory.worldgen, &factory.biomes).map_err(|e| e.to_string())?;
            Ok((Arc::new(FlatLevelSource::new(settings, factory.biomes.clone())), overworld_settings()?))
        }
        "minecraft:debug" => {
            let plains = mc_worldgen::flat::settings::get_default_biome(&factory.biomes).map_err(|e| e.to_string())?;
            Ok((Arc::new(mc_worldgen::debug_level_source::DebugLevelSource::new(plains, factory.biomes.clone())), overworld_settings()?))
        }
        other => Err(format!("Unknown registry key in minecraft:chunk_generator: {other}")),
    }
}

impl MinecraftServer {
    /// `loadLevel`.
    pub(super) fn load_level(&mut self) {
        let brand = self.get_server_mod_name();
        self.world_data.lock().set_modded_info(brand, false);
        self.create_levels();
        self.force_difficulty();
        self.prepare_levels();
    }

    fn level_factory(&self) -> LevelFactory {
        let access = self.registries.composite_access();
        let worldgen = Arc::new(access.worldgen_registries());
        let biomes = Arc::new(BiomeRegistry::load(&worldgen).unwrap_or_else(|e| panic!("Failed to load the biome registry: {e}")));
        let factory = Arc::new(
            PalettedContainerFactory::create(mc_world::palette::BiomeRegistry::new(biomes.names())).expect("the biome registry is not empty"),
        );
        let structures: std::collections::HashSet<String> =
            access.registry(keys::STRUCTURE).map(|r| r.names().iter().map(ToString::to_string).collect()).unwrap_or_default();
        LevelFactory { worldgen, biomes, factory, known_structures: Arc::new(move |id| structures.contains(id)) }
    }

    /// `new ServerLevel(...)` for one `LevelStem`.
    fn create_level(&self, factory: &LevelFactory, dimension: &DimensionKey, stem: &LevelStem, is_debug: bool) -> ServerLevel {
        let access = self.registries.composite_access();
        let dimension_type: Arc<DimensionType> = match &stem.dimension_type {
            DimensionTypeSource::Inline(inline) => inline.clone(),
            DimensionTypeSource::Reference(id) => Arc::new(
                access
                    .get::<DimensionType>(keys::DIMENSION_TYPE, id)
                    .cloned()
                    .unwrap_or_else(|| panic!("Unknown dimension type {id} of dimension {dimension}")),
            ),
        };
        let seed = self.world_gen_settings.options.seed;
        let (chunk_generator, noise_settings) =
            build_generator(&stem.generator, factory).unwrap_or_else(|e| panic!("Failed to create the chunk generator of {dimension}: {e}"));
        let random_state = RandomState::create(factory.worldgen.clone(), seed, &noise_settings);
        let sea_level = chunk_generator.get_sea_level();
        let backend = NoiseWorldGenerator::new(chunk_generator, Arc::new(random_state), seed, Identifier::try_parse(stem.generator_type()));
        let dimension_dir = self
            .storage_source
            .get_dimension_path(dimension)
            .unwrap_or_else(|e| panic!("Failed to resolve the folder of dimension {dimension}: {e}"));
        let is_overworld = *dimension == overworld_key();
        let level_data: Box<dyn ServerLevelData> = if is_overworld {
            Box::new(PrimaryServerLevelData(self.world_data.clone()))
        } else {
            Box::new(DerivedLevelData::new(self.world_data.clone(), Box::new(PrimaryServerLevelData(self.world_data.clone()))))
        };
        let properties = self.properties();
        let enabled_features = self.world_data.lock().settings.data_configuration.enabled_features;
        let mut level = ServerLevel::new(ServerLevelParams {
            shared: self.shared.clone(),
            dimension: dimension.clone(),
            level_id: self.storage_source.get_level_id().to_owned(),
            dimension_dir,
            level_height: LevelHeight { min_y: dimension_type.min_y, height: dimension_type.height },
            sea_level,
            has_sky_light: dimension_type.has_sky_light,
            factory: factory.factory.clone(),
            generator: Arc::new(backend),
            generate_structures: self.world_gen_settings.options.generate_structures,
            seed,
            is_known_structure: factory.known_structures.clone(),
            view_distance: *properties.view_distance.get(),
            simulation_distance: *properties.simulation_distance.get(),
            sync_writes: properties.sync_chunk_writes,
            is_debug,
            level_data,
        });
        level.has_ceiling = dimension_type.has_ceiling;
        level.tick_state.tick_time = is_overworld;
        level.tick_state.can_have_weather = dimension_type.has_sky_light && !dimension_type.has_ceiling;
        level.tick_state.enabled_features = enabled_features;
        level.tick_state.biomes = Some(factory.biomes.clone());
        level.tick_state.biome_zoom_seed = BiomeManager::obfuscate_seed(seed);
        level.chunk_tick_hooks.tick_chunk = Some(|level, pos, speed| level.tick_chunk(pos, speed));
        level
    }

    /// `createLevels`.
    fn create_levels(&mut self) {
        let is_debug = self.world_data.lock().is_debug_world();
        let stems: Vec<(Identifier, Arc<LevelStem>)> = {
            let registry = self.registries.composite_access().registry_or_throw(keys::LEVEL_STEM);
            registry
                .entries()
                .iter()
                .filter_map(|entry| entry.value.as_ref().and_then(|v| v.clone().downcast::<LevelStem>().ok()).map(|stem| (entry.id.clone(), stem)))
                .collect()
        };
        let overworld_key = overworld_key();
        let factory = self.level_factory();
        let overworld_stem = stems.iter().find(|(k, _)| *k == overworld_key).map(|(_, s)| s.clone()).expect("checked by MinecraftServer::new");
        let mut overworld = self.create_level(&factory, &overworld_key, &overworld_stem, is_debug);
        let initialized = self.world_data.lock().initialized;
        if !initialized {
            let spawn_bonus_chest = self.world_gen_settings.options.generate_bonus_chest;
            let result = panic::catch_unwind(AssertUnwindSafe(|| self.set_initial_spawn(&mut overworld, spawn_bonus_chest, is_debug)));
            if let Err(payload) = result {
                let mut report = CrashReport::for_throwable(payload, "Exception initializing level");
                report.add_category("Level").set_detail("Dimension", &overworld_key);
                ReportedException::throw(report);
            }
            self.world_data.lock().initialized = true;
            if is_debug {
                self.setup_debug_level();
            }
        }
        let focus = self.world_data.lock().respawn_data.clone();
        self.level_load_listener.update_focus(&focus.dimension, mc_core::pos::ChunkPos::new(focus.pos.x() >> 4, focus.pos.z() >> 4));
        self.add_level(overworld);

        for (key, stem) in &stems {
            if *key == overworld_key {
                continue;
            }
            let level = self.create_level(&factory, key, stem, is_debug);
            self.add_level(level);
        }
        // `level.getWorldBorder().setAbsoluteMaxSize(..)` and `playerList.addWorldborderListener(level)`: the world
        // border model is not part of the level yet.
    }

    /// `setupDebugLevel`.
    fn setup_debug_level(&mut self) {
        {
            let mut data = self.world_data.lock();
            data.settings.difficulty_settings.difficulty = Difficulty::Peaceful;
            data.settings.difficulty_settings.locked = true;
            data.settings.game_type = GameType::Spectator;
        }
        self.set_game_rule(GameRules::ADVANCE_WEATHER, false);
    }

    /// `prepareLevels`: activate the saved tickets, then wait (running tasks) until every requested chunk is full.
    fn prepare_levels(&mut self) {
        let mut counters: Vec<(DimensionKey, ChunkLoadCounter)> = Vec::new();
        for dimension in self.level_keys() {
            let mut counter = ChunkLoadCounter::new();
            self.with_level(&dimension, |_server, level| counter.track(level, |level| level.chunk_source.activate_all_deactivated_tickets()));
            counters.push((dimension, counter));
        }
        let total: i32 = counters.iter().map(|(_, c)| c.total_chunks()).sum();
        self.level_load_listener.start(Stage::LoadInitialChunks, total);
        loop {
            let mut ready = 0;
            let mut pending = 0;
            for (dimension, counter) in &mut counters {
                if let Some(level) = self.level(dimension) {
                    ready += counter.ready_chunks(level);
                    pending += counter.pending_chunks(level);
                }
            }
            self.level_load_listener.update(Stage::LoadInitialChunks, ready, total);
            self.set_next_tick_time(get_nanos() + PREPARE_LEVELS_DEFAULT_DELAY_NANOS);
            self.wait_until_next_tick();
            if pending == 0 {
                break;
            }
        }
        self.level_load_listener.finish(Stage::LoadInitialChunks);
        self.update_mob_spawning_flags();
        self.update_effective_respawn_data();
    }

    /// `updateMobSpawningFlags`: `level.setSpawnSettings(level.isSpawningMonsters())`.
    pub fn update_mob_spawning_flags(&mut self) {
        let peaceful = self.world_data.lock().settings.difficulty_settings.difficulty == Difficulty::Peaceful;
        let spawning_monsters = !peaceful && self.game_rule(GameRules::SPAWN_MONSTERS);
        for level in self.all_levels_mut() {
            level.chunk_source.set_spawn_settings(spawning_monsters);
        }
    }

    /// `setDifficulty(difficulty, ignoreLock)`.
    pub fn set_difficulty(&mut self, difficulty: Difficulty, ignore_lock: bool) {
        let (locked, hardcore) = {
            let data = self.world_data.lock();
            (data.settings.difficulty_settings.locked, data.settings.difficulty_settings.hardcore)
        };
        if ignore_lock || !locked {
            self.world_data.lock().settings.difficulty_settings.difficulty = if hardcore { Difficulty::Hard } else { difficulty };
            self.update_mob_spawning_flags();
            self.send_difficulty_update_to_all();
        }
    }

    /// `setDifficultyLocked`.
    pub fn set_difficulty_locked(&mut self, locked: bool) {
        self.world_data.lock().settings.difficulty_settings.locked = locked;
        self.send_difficulty_update_to_all();
    }

    fn send_difficulty_update_to_all(&self) {
        let (difficulty, locked) = {
            let data = self.world_data.lock();
            (data.settings.difficulty_settings.difficulty, data.settings.difficulty_settings.locked)
        };
        self.player_list.broadcast_all(&mc_protocol::packets::play::ClientboundChangeDifficultyPacket { difficulty: mc_protocol::packets::play::Difficulty::by_id(difficulty.get_id()), locked });
    }

    /// `updateEffectiveRespawnData`: the respawn data adjusted to the world border (not modelled yet: unchanged).
    pub(super) fn update_effective_respawn_data(&mut self) {
        self.effective_respawn_data = Some(self.world_data.lock().respawn_data.clone());
    }

    /// `findRespawnDimension`: the dimension of the overworld level data's respawn, else the overworld.
    pub fn find_respawn_dimension(&self) -> DimensionKey {
        let dimension = self.world_data.lock().respawn_data.dimension.clone();
        if self.level(&dimension).is_some() { dimension } else { overworld_key() }
    }

    /// `getRespawnData`.
    pub fn get_respawn_data(&self) -> RespawnData {
        self.effective_respawn_data.clone().unwrap_or_else(|| self.world_data.lock().respawn_data.clone())
    }

    /// `setRespawnData`.
    pub fn set_respawn_data(&mut self, respawn_data: RespawnData) {
        let changed = self.world_data.lock().respawn_data != respawn_data;
        if changed {
            self.world_data.lock().respawn_data = respawn_data.clone();
            self.player_list.broadcast_all(&mc_protocol::packets::play::ClientboundSetDefaultSpawnPositionPacket {
                respawn_data: mc_protocol::packets::play::RespawnData { global_pos: mc_protocol::codec::GlobalPos { dimension: respawn_data.dimension, pos: respawn_data.pos }, yaw: respawn_data.yaw, pitch: respawn_data.pitch },
            });
            self.update_effective_respawn_data();
        }
    }

    // ------------------------------------------------------------------ tickChildren

    /// `tickChildren`.
    pub(super) fn tick_children(&mut self, have_time: &mut dyn FnMut() -> bool) {
        if self.tick_rate_manager.runs_normally() {
            self.tick_clocks();
        }
        if self.tick_count % 20 == 0 {
            self.force_game_time_synchronization();
        }

        self.update_effective_respawn_data();
        for dimension in self.level_keys() {
            let result = self.with_level(&dimension, |server, level| {
                let mut env = ServerTickEnv { server, dimension: dimension.clone() };
                panic::catch_unwind(AssertUnwindSafe(|| level.tick(&mut env, &mut *have_time))).map_err(|payload| {
                    let mut report = CrashReport::for_throwable(payload, "Exception ticking world");
                    report.add_category("Level").set_detail("Dimension", &dimension);
                    report
                })
            });
            if let Some(Err(report)) = result {
                ReportedException::throw(report);
            }
        }

        self.tick_connection();
        self.player_list.tick();
        self.run_tickables();
    }

    /// `forceGameTimeSynchronization`.
    pub fn force_game_time_synchronization(&mut self) {
        let game_time = self.world_data.lock().game_time;
        self.player_list.broadcast_all(&mc_protocol::packets::play::ClientboundSetTimePacket { game_time, clock_updates: Vec::new() });
    }

    // ------------------------------------------------------------------ saving

    /// `saveAllChunks` (+ the notifications of `DedicatedServer.saveAllChunks`).
    pub fn save_all_chunks(&mut self, silent: bool, flush: bool, force: bool) -> bool {
        self.notification_manager.server_save_started();
        let level_name = self.world_data.lock().settings.level_name.clone();
        let mut result = false;
        for level in self.all_levels_mut() {
            if !silent {
                log::info!("Saving chunks for level 'ServerLevel[{level_name}]'/{}", level.dimension);
            }
            if !(level.no_save && !force) {
                level.save_chunks(flush);
            }
            result = true;
        }
        let (data, uuid) = {
            let data = self.world_data.lock();
            (data.clone(), data.single_player_uuid)
        };
        if let Err(e) = self.storage_source.save_data_tag(&data, uuid) {
            log::error!("Failed to save level {}: {e}", self.storage_source.get_level_id());
        }
        if self.world_gen_settings_dirty {
            self.world_gen_settings_dirty = false;
            let root = self.storage_source.get_level_path(mc_world::level_storage::LevelResource::ROOT);
            if let Err(e) = self.world_gen_settings.write(&root) {
                log::error!("Could not save data world_gen_settings: {e}");
            }
        }
        if flush {
            self.saved_data.save_and_join();
        } else {
            let pending = self.saved_data.schedule_save();
            if !pending.is_empty() {
                std::thread::Builder::new().name("IO-Worker".to_owned()).spawn(move || pending.write()).ok();
            }
        }
        self.notification_manager.server_save_completed();
        result
    }

    /// `saveEverything`.
    pub fn save_everything(&mut self, silent: bool, flush: bool, force: bool) -> bool {
        self.shared.set_saving(true);
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let result = self.save_all_chunks(silent, flush, force);
            self.warn_on_low_disk_space();
            result
        }));
        self.shared.set_saving(false);
        result.unwrap_or_else(|payload| panic::resume_unwind(payload))
    }

    /// `setAutoSave`: `/save-on`, `/save-off` (`level.noSave`).
    pub fn set_auto_save(&mut self, enable: bool) -> bool {
        let mut success = false;
        for level in self.all_levels_mut() {
            if level.no_save == enable {
                level.no_save = !enable;
                level.chunk_source.chunk_map.settings.no_save = !enable;
                success = true;
            }
        }
        success
    }

    /// `isAutoSave`.
    pub fn is_auto_save(&self) -> bool {
        self.all_levels().any(|level| !level.no_save)
    }

    /// `warnOnLowDiskSpace`.
    pub(super) fn warn_on_low_disk_space(&mut self) {
        if self.storage_source.check_for_low_disk_space() {
            self.send_low_disk_space_warning();
        }
    }

    /// `sendLowDiskSpaceWarning` (`DedicatedServer`: also tells every operator).
    fn send_low_disk_space_warning(&mut self) {
        log::warn!("Low disk space! Might not be able to save the world.");
        for player in self.player_list.get_players() {
            if self.player_list.is_op(&player.name_and_id()) {
                player.connection.send(&mc_protocol::packets::play::ClientboundLowDiskSpaceWarningPacket);
            }
        }
    }

    // ------------------------------------------------------------------ stopServer

    /// `stopServer` (+ `DedicatedServer.stopServer`: shutting-down notification first).
    pub(super) fn stop_server(&mut self) {
        self.notification_manager.server_shutting_down();
        log::info!("Stopping server");
        self.stop_connection();
        self.shared.set_saving(true);
        log::info!("Saving players");
        self.player_list.remove_all();

        log::info!("Saving worlds");
        for level in self.all_levels_mut() {
            level.no_save = false;
            level.chunk_source.chunk_map.settings.no_save = false;
        }

        while self.all_levels().any(|level| level.chunk_source.chunk_map.has_work()) {
            self.set_next_tick_time(get_nanos() + tick_pacing::NANOSECONDS_PER_MILLISECOND);
            let runs_normally = self.tick_rate_manager.runs_normally();
            for level in self.all_levels_mut() {
                level.chunk_source.deactivate_tickets_on_closing();
                level.tick_chunk_source(&|| true, false, runs_normally);
            }
            self.wait_until_next_tick();
        }

        self.save_all_chunks(false, true, false);

        for level in self.all_levels_mut() {
            level.close();
        }

        self.shared.set_saving(false);
        self.saved_data.close();
        self.resources.resource_manager.close();
        if let Err(e) = self.storage_source.close() {
            log::error!("Failed to unlock level {}\n{e}", self.storage_source.get_level_id());
        }
    }
}
