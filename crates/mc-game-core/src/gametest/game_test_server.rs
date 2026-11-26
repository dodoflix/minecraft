//! `GameTestServer`: the headless server of `net.minecraft.gametest.Main`. It is the same `MinecraftServer` as the
//! dedicated one with the overrides of `GameTestServer` (see `DedicatedServer::game_test`): a fixed world (flat in every
//! dimension, seed 0, creative, "Test Level"), no console, network or properties, a tick loop that never waits, and a
//! process exit code that is the number of failed required tests.

use super::game_test_batch::{GameTestBatch, to_game_test_batch};
use super::game_test_info::GameTestInfo;
use super::game_test_instance::{TestHolder, all_tests, lookup_test};
use super::game_test_runner;
use super::global_test_reporter;
use super::multiple_test_tracker::MultipleTestTracker;
use super::retry_options::RetryOptions;
use super::structure_grid_spawner::StructureGridSpawner;
use crate::commands::CommandBuildContext;
use crate::commands::arguments::resource_arguments::ResourceSelectorArgument;
use crate::commands::command_build_context::AccessRegistries;
use crate::crash_report::{CrashReport, ReportType, install_panic_hook};
use crate::server::MinecraftServer;
use crate::server::dedicated::console::ConsoleInputQueue;
use crate::server::dedicated::dedicated_server::DedicatedServer;
use crate::server::dedicated::launch::{WorldDataAndGenSettings, preset_dimensions};
use crate::server::dedicated::properties::DedicatedServerSettings;
use crate::server::dedicated::stop::ServerStopSignal;
use crate::server::level_load_listener::LoggingLevelLoadListener;
use crate::server::minecraft_server::{ReloadableResources, SERVER_THREAD_NAME, ServerInit};
use crate::server::notifications::NotificationManager;
use crate::server::packs::ServerPacksSource;
use crate::server::permissions::LevelBasedPermissionSet;
use crate::server::players::name_resolver::{
    CachedProfileResolver, CachedUserNameToIdResolver, NoProfileLookup,
};
use crate::server::services_keys::ServicesKeys;
use crate::server::shared::ServerShared;
use crate::server::world_dimensions::DimensionDecoder;
use crate::server::world_loader::{DataLoadOutput, PackConfig, WorldStem, load};
use crate::world::level::DimensionKey;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_core::rotation::Rotation;
use mc_world::level_storage::{
    DataPackConfig, DifficultySettings, FeatureFlagSet, GameType, LevelResource, LevelSettings,
    LevelStorageSource, PrimaryLevelData, RespawnData, WorldDataConfiguration, WorldGenSettings,
    WorldOptions,
};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// `ENABLED_FEATURES`: every feature flag but `REDSTONE_EXPERIMENTS` and `MINECART_IMPROVEMENTS`.
const ENABLED_FEATURES: FeatureFlagSet =
    FeatureFlagSet::VANILLA.join(FeatureFlagSet::TRADE_REBALANCE);
/// `TEST_POSITION_RANGE`.
const TEST_POSITION_RANGE: i32 = 14_999_992;
/// `PROGRESS_REPORT_INTERVAL`.
const PROGRESS_REPORT_INTERVAL: i64 = 20;
/// `LEVEL_NAME` of `GameTestMainUtil`.
pub const LEVEL_NAME: &str = "gametestworld";

/// The options `GameTestMainUtil` hands to `GameTestServer.create`.
#[derive(Clone, Debug)]
pub struct GameTestConfig {
    /// `--tests`.
    pub selection: Option<String>,
    /// `--verify`.
    pub verify: bool,
    /// `--repeatCount`.
    pub repeat_count: i32,
}

/// What the server thread keeps of `GameTestServer` besides its configuration (the tests are `Rc`, so thread local).
#[derive(Default)]
struct State {
    test_batches: Vec<GameTestBatch>,
    test_tracker: Option<Rc<MultipleTestTracker>>,
    stopwatch: Option<Instant>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// `GameTestServer.create` + `MinecraftServer.spin`: loads the world and runs the server on its own thread until the
/// process exits (`onServerExit` ends in `System.exit`). Returns only when the server could not start; `config` is
/// `None` (and the exit code 0) when reading the options failed.
pub fn spin(universe: &Path, config: impl FnOnce() -> Option<GameTestConfig>) -> i32 {
    install_panic_hook();
    let level_storage_source = match LevelStorageSource::create_default(universe) {
        Ok(source) => source,
        Err(e) => {
            log::error!("Failed to create the level storage: {e}");
            return 1;
        }
    };
    let access = match level_storage_source.create_access(LEVEL_NAME) {
        Ok(access) => access,
        Err(e) => {
            log::error!("Failed to create the level storage access: {e}");
            return 1;
        }
    };
    let mut pack_repository = ServerPacksSource::create_pack_repository(
        access.get_level_path(LevelResource::DATAPACK_DIR),
        level_storage_source.get_world_dir_validator().clone(),
    );
    // the arguments of `GameTestServer.create(..)` are read when the server thread starts, after the storage access
    let Some(config) = config() else {
        return 0;
    };
    // `GameTestServer.create`
    pack_repository.reload();
    let mut enabled_packs: Vec<String> =
        pack_repository.available_ids().map(str::to_owned).collect();
    enabled_packs.retain(|id| id != "vanilla");
    enabled_packs.insert(0, "vanilla".to_owned());
    let default_test_config = WorldDataConfiguration {
        data_packs: Some(DataPackConfig {
            enabled: enabled_packs,
            disabled: Vec::new(),
        }),
        enabled_features: ENABLED_FEATURES,
    };
    let test_settings = LevelSettings {
        level_name: "Test Level".to_owned(),
        game_type: GameType::Creative,
        difficulty_settings: DifficultySettings::DEFAULT,
        allow_commands: true,
        data_configuration: default_test_config.clone(),
    };
    let mut pack_config = PackConfig {
        pack_repository: &mut pack_repository,
        initial_data_config: default_test_config,
        safe_mode: false,
        init_mode: true,
        function_compilation_permissions: LevelBasedPermissionSet::OWNER.into(),
    };
    log::debug!("Starting resource loading");
    let started = Instant::now();
    let loaded = load(
        &mut pack_config,
        |context| {
            let decoder = DimensionDecoder::new(&context.datapack_world_registries)
                .unwrap_or_else(|e| panic!("{e}"));
            let dimensions = preset_dimensions(&decoder, context, "minecraft:flat_all_dimensions")
                .unwrap_or_else(|| panic!("Missing world preset minecraft:flat_all_dimensions"));
            // `bake(noDatapackDimensions)`: every dimension comes from the preset
            let complete = dimensions
                .bake(&decoder, None)
                .unwrap_or_else(|e| panic!("{e}"));
            let data =
                PrimaryLevelData::new(test_settings.clone(), complete.special_world_property);
            Ok(DataLoadOutput {
                cookie: WorldDataAndGenSettings {
                    data,
                    gen_settings: WorldGenSettings {
                        options: WorldOptions::new(0, false, false),
                        dimensions: dimensions.to_stored(),
                    },
                    lifecycle: complete.lifecycle,
                },
                final_dimensions: complete.dimensions,
            })
        },
        WorldStem::new,
    );
    let (_selected_config, world_stem) = match loaded {
        Ok(loaded) => loaded,
        Err(e) => {
            log::warn!("Failed to load vanilla datapack, bit oops\n{e}");
            exit(-1)
        }
    };
    log::debug!(
        "Finished resource loading after {} ms",
        started.elapsed().as_millis()
    );
    let WorldStem {
        resource_manager,
        data_pack_resources,
        registries,
        world_data,
    } = world_stem;
    let player_data_storage = match access.create_player_storage() {
        Ok(storage) => storage,
        Err(e) => {
            log::error!("Failed to create the player data storage: {e}");
            return 1;
        }
    };
    let lookup = Arc::new(NoProfileLookup);
    let name_to_id_cache = Arc::new(CachedUserNameToIdResolver::new(
        lookup.clone(),
        universe.join("usercache.json"),
    ));
    let profile_resolver = Arc::new(CachedProfileResolver::new(
        lookup.clone(),
        name_to_id_cache.clone(),
    ));
    let mut dedicated = match DedicatedServer::new(
        DedicatedServerSettings::detached(),
        ConsoleInputQueue::new(),
        ServerStopSignal::new(),
        lookup,
        ServicesKeys::unavailable(),
    ) {
        Ok(dedicated) => dedicated,
        Err(e) => {
            log::error!("Failed to create the server: {e}");
            return 1;
        }
    };
    dedicated.game_test = Some(config);
    let (shared, tasks) = ServerShared::new();
    let init = ServerInit {
        server_directory: std::path::PathBuf::new(),
        storage_source: access,
        player_data_storage,
        world_data: world_data.data,
        world_gen_settings: world_data.gen_settings,
        world_gen_settings_lifecycle: world_data.lifecycle,
        pack_repository,
        resources: ReloadableResources {
            resource_manager,
            managers: data_pack_resources,
        },
        registries,
        dedicated,
        notification_manager: Arc::new(NotificationManager::new()),
        name_to_id_cache,
        profile_resolver,
        level_load_listener: Box::new(LoggingLevelLoadListener::for_dedicated_server()),
    };
    let thread = std::thread::Builder::new()
        .name(SERVER_THREAD_NAME.to_owned())
        .spawn(move || {
            install_panic_hook();
            match MinecraftServer::new(shared, tasks, init) {
                Ok(mut server) => server.run_server(),
                Err(message) => log::error!(
                    "Failed to start the minecraft server\njava.lang.IllegalStateException: {message}"
                ),
            }
        });
    match thread.map(|t| t.join()) {
        Ok(Ok(())) => 0,
        _ => 1,
    }
}

/// `System.exit(code)`: the log lines are on disk and in the console first.
fn exit(code: i32) -> ! {
    log::logger().flush();
    std::process::exit(code)
}

/// `GameTestServer.initServer`: the level, then the tests to run.
pub fn init_server(server: &mut MinecraftServer, config: &GameTestConfig) -> bool {
    server.load_level();
    let batches = evaluate_tests_to_run(server, config);
    STATE.with(|s| s.borrow_mut().test_batches = batches);
    log::info!("Started game test server");
    true
}

/// `evaluateTestsToRun`.
fn evaluate_tests_to_run(
    server: &mut MinecraftServer,
    config: &GameTestConfig,
) -> Vec<GameTestBatch> {
    let registries = server.registries.composite_access().clone();
    let tests: Vec<TestHolder>;
    type Decorator = Box<dyn Fn(&TestHolder, &DimensionKey) -> Vec<Rc<GameTestInfo>>>;
    let decorator: Decorator;
    if let Some(selection) = &config.selection {
        tests = get_tests_for_selection(server, selection)
            .into_iter()
            .filter(|test| !test.value.manual_only())
            .collect();
        if tests.is_empty() {
            log::warn!("Test selection matcher ({selection}) found no tests");
            exit(-1);
        }
        if config.verify {
            decorator = Box::new(rotate_and_multiply);
            log::info!(
                "Verify requested. Will run each test that matches {selection} {} times",
                100 * Rotation::ALL.len()
            );
        } else if config.repeat_count > 0 {
            let repeat_count = config.repeat_count;
            decorator =
                Box::new(move |test, dimension| multiply_test(repeat_count, test, dimension));
            log::info!(
                "Each test that matches {selection} will be run {} times (total: {})",
                config.repeat_count,
                (tests.len() as i32).wrapping_mul(config.repeat_count)
            );
        } else {
            decorator = Box::new(direct);
            log::info!(
                "Will run tests matching {selection} ({} tests)",
                tests.len()
            );
        }
    } else {
        tests = all_tests(&registries)
            .into_iter()
            .filter(|test| !test.value.manual_only())
            .collect();
        decorator = Box::new(direct);
    }
    divide_into_batches(server, &tests, &*decorator)
}

/// `getTestsForSelection(registries, selection)`.
fn get_tests_for_selection(server: &MinecraftServer, selection: &str) -> Vec<TestHolder> {
    let registries = server.registries.composite_access();
    let context = CommandBuildContext::simple(
        Arc::new(AccessRegistries(registries.clone())),
        ENABLED_FEATURES,
    );
    let argument = ResourceSelectorArgument::resource_selector(&context, "test_instance");
    let (_, selected) = argument.select(&mut mc_brigadier::StringReader::new(selection));
    selected
        .iter()
        .filter_map(|key| lookup_test(registries, &key.id))
        .collect()
}

/// `GameTestBatchFactory.DIRECT`.
fn direct(test: &TestHolder, dimension: &DimensionKey) -> Vec<Rc<GameTestInfo>> {
    vec![super::game_test_batch::direct(
        test.clone(),
        dimension.clone(),
    )]
}

/// `rotateAndMultiply`: 100 copies of the test for every rotation.
fn rotate_and_multiply(test: &TestHolder, dimension: &DimensionKey) -> Vec<Rc<GameTestInfo>> {
    let mut infos = Vec::new();
    for rotation in Rotation::ALL {
        for _ in 0..100 {
            infos.push(GameTestInfo::new(
                test.clone(),
                rotation,
                dimension.clone(),
                RetryOptions::no_retries(),
            ));
        }
    }
    infos
}

/// `multiplyTest`.
fn multiply_test(
    repeat_count: i32,
    test: &TestHolder,
    dimension: &DimensionKey,
) -> Vec<Rc<GameTestInfo>> {
    (0..repeat_count)
        .map(|_| {
            GameTestInfo::new(
                test.clone(),
                Rotation::None,
                dimension.clone(),
                RetryOptions::no_retries(),
            )
        })
        .collect()
}

/// `GameTestBatchFactory.divideIntoBatches`: the tests grouped by (environment, dimension) in first-seen order (vanilla
/// groups through a `HashMap`), every group decorated and split into batches of at most 50.
fn divide_into_batches(
    server: &MinecraftServer,
    all_tests: &[TestHolder],
    decorator: &dyn Fn(&TestHolder, &DimensionKey) -> Vec<Rc<GameTestInfo>>,
) -> Vec<GameTestBatch> {
    let mut groups: Vec<(_, DimensionKey, Vec<&TestHolder>)> = Vec::new();
    for test in all_tests {
        let (environment, dimension) = (
            test.value.batch().clone(),
            test.value.info().dimension.clone(),
        );
        match groups
            .iter_mut()
            .find(|(e, d, _)| *e == environment && *d == dimension)
        {
            Some((_, _, members)) => members.push(test),
            None => groups.push((environment, dimension, vec![test])),
        }
    }
    let mut batches = Vec::new();
    for (environment, dimension, members) in groups {
        assert!(
            server.level(&dimension).is_some(),
            "Missing level for dimension: {dimension}"
        );
        let in_batch: Vec<Rc<GameTestInfo>> = members
            .into_iter()
            .flat_map(|test| decorator(test, &dimension))
            .collect();
        for (index, tests) in in_batch.chunks(50).enumerate() {
            batches.push(to_game_test_batch(
                tests.to_vec(),
                environment.clone(),
                index as i32,
                dimension.clone(),
            ));
        }
    }
    batches
}

/// `GameTestServer.tickServer` after `super.tickServer`.
pub fn tick_server(server: &mut MinecraftServer) {
    if STATE.with(|s| s.borrow().test_tracker.is_none()) {
        start_tests(server);
    }
    let tracker = STATE
        .with(|s| s.borrow().test_tracker.clone())
        .expect("the tests have started");
    if server.overworld().game_time() % PROGRESS_REPORT_INTERVAL == 0 {
        log::info!("{}", tracker.get_progress_bar());
    }
    if tracker.is_done() {
        server.shared.halt();
        log::info!("{}", tracker.get_progress_bar());
        global_test_reporter::finish();
        let elapsed = STATE
            .with(|s| s.borrow().stopwatch)
            .map(|started| started.elapsed());
        log::info!(
            "========= {} GAME TESTS COMPLETE IN {} ======================",
            tracker.get_total_count(),
            stopwatch_to_string(elapsed.map_or(0, |e| e.as_nanos()))
        );
        if tracker.has_failed_required() {
            log::info!(
                "{} required tests failed :(",
                tracker.get_failed_required_count()
            );
            for test in tracker.get_failed_required() {
                log_failed_test(&test);
            }
        } else {
            log::info!("All {} required tests passed :)", tracker.get_total_count());
        }
        if tracker.has_failed_optional() {
            log::info!(
                "{} optional tests failed",
                tracker.get_failed_optional_count()
            );
            for test in tracker.get_failed_optional() {
                log_failed_test(&test);
            }
        }
        log::info!("====================================================");
    }
}

/// `logFailedTest`.
fn log_failed_test(test_info: &Rc<GameTestInfo>) {
    let description = test_info
        .get_error()
        .map(|e| e.description().get_string())
        .unwrap_or_default();
    if test_info.get_rotation() != Rotation::None {
        log::info!(
            "   - {} with rotation {}: {}",
            test_info.id(),
            test_info.get_rotation().get_serialized_name(),
            description
        );
    } else {
        log::info!("   - {}: {}", test_info.id(), description);
    }
}

/// `startTests(level)`.
fn start_tests(server: &mut MinecraftServer) {
    let (x, z) = {
        let random = &mut server.overworld_mut().tick_state.random;
        (
            random.next_int_between_inclusive(-TEST_POSITION_RANGE, TEST_POSITION_RANGE),
            random.next_int_between_inclusive(-TEST_POSITION_RANGE, TEST_POSITION_RANGE),
        )
    };
    let start_pos = BlockPos::new(x, 4, z);
    let dimension = server.overworld().dimension.clone();
    server.set_respawn_data(RespawnData {
        dimension,
        pos: start_pos,
        yaw: 0.0,
        pitch: 0.0,
    });
    let batches = STATE.with(|s| std::mem::take(&mut s.borrow_mut().test_batches));
    let runner = game_test_runner::Builder::from_batches(batches)
        .new_structure_spawner(StructureGridSpawner::new(
            Rc::new(move |_, _| start_pos),
            8,
            false,
        ))
        .build();
    let tracker = MultipleTestTracker::with_tests(runner.get_test_infos());
    log::info!(
        "{} tests are now running at position {}!",
        tracker.get_total_count(),
        start_pos.to_short_string()
    );
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.test_tracker = Some(tracker);
        state.stopwatch = Some(Instant::now());
    });
    runner.start(server);
}

/// `onServerExit`: `System.exit(failedRequiredCount)`, `-1` when the tests never started.
pub fn on_server_exit(_server: &mut MinecraftServer) {
    log::info!("Game test server shutting down");
    let code = STATE.with(|s| {
        s.borrow()
            .test_tracker
            .as_ref()
            .map_or(-1, |t| t.get_failed_required_count())
    });
    exit(code)
}

/// `onServerCrash`: `System.exit(1)`.
pub fn on_server_crash(report: &CrashReport) {
    log::error!(
        "Game test server crashed\n{}",
        report.get_friendly_report(ReportType::CRASH)
    );
    exit(1)
}

/// Guava `Stopwatch.toString()`: the elapsed time in the largest unit it fills, four significant digits.
fn stopwatch_to_string(nanos: u128) -> String {
    const UNITS: [(u128, &str); 7] = [
        (86_400_000_000_000, "d"),
        (3_600_000_000_000, "h"),
        (60_000_000_000, "min"),
        (1_000_000_000, "s"),
        (1_000_000, "ms"),
        (1_000, "\u{3bc}s"),
        (1, "ns"),
    ];
    let (size, abbreviation) = UNITS
        .iter()
        .find(|(size, _)| nanos >= *size)
        .copied()
        .unwrap_or((1, "ns"));
    let value = nanos as f64 / size as f64;
    // `%.4g`: four significant digits, never scientific for these magnitudes
    let integer_digits = if value < 1.0 {
        1
    } else {
        value.log10().floor() as usize + 1
    };
    let decimals = 4usize.saturating_sub(integer_digits);
    format!("{value:.decimals$} {abbreviation}")
}

#[cfg(test)]
mod tests {
    use super::stopwatch_to_string;

    #[test]
    fn the_stopwatch_text_has_four_significant_digits() {
        assert_eq!(stopwatch_to_string(10_490_000_000), "10.49 s");
        assert_eq!(stopwatch_to_string(2_000_000_000), "2.000 s");
        assert_eq!(stopwatch_to_string(852_000), "852.0 \u{3bc}s");
        assert_eq!(stopwatch_to_string(1_234_000), "1.234 ms");
        assert_eq!(stopwatch_to_string(0), "0.000 ns");
    }
}
