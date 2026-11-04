//! `MinecraftServer`: owned by the game thread. Port of `net.minecraft.server.MinecraftServer` (the dedicated server
//! is the only implementation, so `DedicatedServer`'s overrides are inherent methods in
//! `dedicated::dedicated_server`).
//!
//! This file: struct + construction, level container, the task queue semantics (`execute`/`pollTask`/`managedBlock`),
//! the 20 TPS loop (`runServer`, `waitUntilNextTick`, `tickServer`), autosave, status, crash handling.
//! `level_lifecycle.rs` holds everything that touches the levels (create/prepare/tick/save/stop).

use super::dedicated::dedicated_server::DedicatedServer;
use super::event_loop::TaskQueue;
use super::level_load_listener::LevelLoadListener;
use super::notifications::NotificationManager;
use super::packs::{MultiPackResourceManager, PackRepository};
use super::players::PlayerList;
use super::players::name_resolver::CachedUserNameToIdResolver;
use super::registry_layer::RegistryLayer;
use super::reloadable_resources::ReloadableServerResources;
use super::server_state::SavedDataTypes;
use super::shared::{ServerShared, TickTask, get_nanos};
use super::status::load_status_icon;
use super::tick_pacing::{self, TickTimes};
use super::watchdog::{SharedWatchdogInfo, WatchdogInfo};
use crate::crash_report::{CrashReport, ReportedException, SystemReport, install_panic_hook};
use crate::registry::LayeredRegistryAccess;
use crate::server::level::ServerLevel;
use crate::server::tick_rate_manager::ServerTickRateManager;
use crate::world::level::DimensionKey;
use crate::world::level::gamerules::{GameRules, GameRuleValue, GameRuleValueType};
use crate::world::level::saveddata::{SavedDataManager, SavedDataType};
use indexmap::IndexMap;
use mc_core::random::LegacyRandomSource;
use mc_protocol::auth::ServerKeyPair;
use crate::server::level::level_data::SharedWorldData;
use mc_world::level_storage::{LevelStorageAccess, PlayerDataStorage, PrimaryLevelData, RespawnData, WorldGenSettings};
use parking_lot::Mutex;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// `MinecraftServer.VANILLA_BRAND`.
pub const VANILLA_BRAND: &str = "vanilla";
/// `MinecraftServer.SERVER_THREAD_NAME`.
pub const SERVER_THREAD_NAME: &str = "Server thread";
/// `MinecraftServer.ABSOLUTE_MAX_WORLD_SIZE`.
pub const ABSOLUTE_MAX_WORLD_SIZE: i32 = 29_999_984;
/// `MinecraftServer.MAX_TICK_LATENCY`/`shouldRun`: a task older than this many ticks runs regardless of the budget.
const MAX_TICK_LATENCY: i32 = 3;
/// `lastServerStatus = 0` in vanilla means "rebuild at the next tick" because `Util.getNanos()` is large; ours counts
/// from process start, so the equivalent sentinel is one expiry period in the past.
const STATUS_INVALID: i64 = -tick_pacing::STATUS_EXPIRE_TIME_NANOS;

/// The loaded resources of `MinecraftServer.ReloadableResources`.
pub struct ReloadableResources {
    pub resource_manager: MultiPackResourceManager,
    pub managers: ReloadableServerResources,
}

/// Everything `MinecraftServer`'s constructor takes from the launcher.
pub struct ServerInit {
    pub server_directory: PathBuf,
    pub storage_source: LevelStorageAccess,
    pub player_data_storage: PlayerDataStorage,
    pub world_data: PrimaryLevelData,
    pub world_gen_settings: WorldGenSettings,
    pub pack_repository: PackRepository,
    pub resources: ReloadableResources,
    pub registries: LayeredRegistryAccess<RegistryLayer>,
    pub dedicated: DedicatedServer,
    pub notification_manager: Arc<NotificationManager>,
    pub name_to_id_cache: Arc<CachedUserNameToIdResolver>,
    pub level_load_listener: Box<dyn LevelLoadListener>,
}

pub struct MinecraftServer {
    pub shared: Arc<ServerShared>,
    tasks: TaskQueue,
    /// `levels` in vanilla iteration order. A level is `None` only while it is taken out for its own tick
    /// (see [`MinecraftServer::with_level`]).
    levels: IndexMap<DimensionKey, Option<Box<ServerLevel>>>,
    /// `tickCount` (mirrored into `shared`; only this struct writes it).
    pub tick_count: i32,

    // --- world storage / data ---
    pub server_directory: PathBuf,
    pub storage_source: LevelStorageAccess,
    pub player_data_storage: PlayerDataStorage,
    /// `WorldData` (the `PrimaryLevelData` of `level.dat`), shared with the levels' `ServerLevelData` views.
    pub world_data: SharedWorldData,
    pub world_gen_settings: WorldGenSettings,
    /// `world_gen_settings.dat` still has to be written (vanilla: the saved data is dirty after `set`).
    pub(super) world_gen_settings_dirty: bool,
    /// `savedDataStorage`: the root `data/` folder (game rules, weather, clocks, random sequences, ...).
    pub saved_data: SavedDataManager,
    pub(super) game_rules_type: SavedDataType<GameRules>,
    pub(super) saved_types: SavedDataTypes,
    pub registries: LayeredRegistryAccess<RegistryLayer>,
    pub resources: ReloadableResources,
    pub pack_repository: PackRepository,
    pub level_load_listener: Box<dyn LevelLoadListener>,
    pub tick_rate_manager: ServerTickRateManager,
    pub player_list: PlayerList,
    pub notification_manager: Arc<NotificationManager>,
    pub name_to_id_cache: Arc<CachedUserNameToIdResolver>,
    pub dedicated: DedicatedServer,
    pub key_pair: Option<Arc<ServerKeyPair>>,
    /// `getConnection()`: the TCP listener and the live connections (`None` before `initServer` / after `stop`).
    pub connection: Option<super::network::ServerConnectionListener>,
    /// What the connection tasks read from the server (settings snapshot, configuration data, player-list side).
    pub network_hooks: Option<Arc<super::network::server_network_hooks::ServerNetworkHooks>>,
    /// Published for the watchdog thread (it cannot read game-thread state).
    pub(super) watchdog_info: SharedWatchdogInfo,

    // --- tick loop (`runServer`) ---
    next_tick_time_nanos: i64,
    last_tick_nanos: i64,
    last_overload_warning_nanos: i64,
    waiting_for_next_tick: bool,
    delayed_tasks_max_next_tick_time_nanos: i64,
    may_have_delayed_tasks: bool,
    tick_times: TickTimes,
    ticks_until_autosave: i32,
    empty_ticks: i32,
    last_server_status: i64,
    /// `statusIcon` (raw PNG).
    status_icon: Option<Vec<u8>>,
    /// `MinecraftServer.random` (status player sample).
    random: LegacyRandomSource,
    /// `tickables` (`addTickable`).
    tickables: Vec<Box<dyn FnMut() + Send>>,

    // --- settings set during `initServer` / by the launcher ---
    pub local_ip: String,
    pub port: i32,
    pub online_mode: bool,
    pub prevent_proxy_connections: bool,
    pub is_demo: bool,
    pub server_id: Option<String>,
    /// `effectiveRespawnData`.
    pub(super) effective_respawn_data: Option<RespawnData>,
}

/// The `PlayerList` settings mirrored from `server.properties` (`uses_authentication` is set by `initServer`).
pub(super) fn player_list_settings(properties: &super::dedicated::properties::DedicatedServerProperties) -> crate::server::players::player_list::PlayerListSettings {
    crate::server::players::player_list::PlayerListSettings {
        max_players: *properties.max_players.get(),
        using_whitelist: *properties.white_list.get(),
        enforce_secure_profile: properties.enforce_secure_profile && properties.online_mode,
        uses_authentication: properties.online_mode,
        log_ips: properties.log_ips,
        operator_user_permissions: properties.op_permissions.get().clone(),
    }
}

impl MinecraftServer {
    /// The `MinecraftServer` constructor. Runs on the game thread, which it binds as the server thread
    /// (`MinecraftServer.spin` creates the thread). `Err` = vanilla's `IllegalStateException`.
    pub fn new(shared: Arc<ServerShared>, tasks: Receiver<TickTask>, init: ServerInit) -> Result<Self, String> {
        let overworld = super::world_dimensions::overworld_key();
        if !init.registries.composite_access().registry_or_throw(crate::registry::keys::LEVEL_STEM).contains(&overworld) {
            return Err("Missing Overworld dimension data".to_owned());
        }
        shared.bind_game_thread();
        let enabled_features = init.world_data.settings.data_configuration.enabled_features;
        let mut saved_data = SavedDataManager::new(mc_world::level_storage::SavedDataStorage::new(
            init.storage_source.get_level_path(mc_world::level_storage::LevelResource::DATA),
        ));
        let game_rules_type = GameRules::type_(enabled_features);
        saved_data.compute_if_absent(&game_rules_type);
        let now = get_nanos();
        let player_list = crate::server::players::DedicatedPlayerList::new(
            player_list_settings(init.dedicated.settings.get_properties()),
            init.player_data_storage.clone(),
            init.storage_source.get_level_path(mc_world::level_storage::LevelResource::PLAYER_STATS_DIR),
            init.storage_source.get_level_path(mc_world::level_storage::LevelResource::PLAYER_ADVANCEMENTS_DIR),
            init.notification_manager.clone(),
            *init.dedicated.settings.get_properties().view_distance.get(),
            *init.dedicated.settings.get_properties().simulation_distance.get(),
        );
        let mut server = Self {
            shared,
            tasks: TaskQueue::new(tasks),
            levels: IndexMap::new(),
            tick_count: 0,
            server_directory: init.server_directory,
            storage_source: init.storage_source,
            player_data_storage: init.player_data_storage,
            world_data: Arc::new(Mutex::new(init.world_data)),
            world_gen_settings: init.world_gen_settings,
            world_gen_settings_dirty: true,
            saved_data,
            game_rules_type,
            saved_types: SavedDataTypes::new(),
            registries: init.registries,
            resources: init.resources,
            pack_repository: init.pack_repository,
            level_load_listener: init.level_load_listener,
            tick_rate_manager: ServerTickRateManager::new(),
            player_list,
            notification_manager: init.notification_manager,
            name_to_id_cache: init.name_to_id_cache,
            dedicated: init.dedicated,
            key_pair: None,
            connection: None,
            network_hooks: None,
            watchdog_info: Arc::new(Mutex::new(WatchdogInfo::default())),
            next_tick_time_nanos: now,
            last_tick_nanos: now,
            last_overload_warning_nanos: i64::MIN / 2,
            waiting_for_next_tick: false,
            delayed_tasks_max_next_tick_time_nanos: 0,
            may_have_delayed_tasks: false,
            tick_times: TickTimes::default(),
            ticks_until_autosave: tick_pacing::AUTOSAVE_INTERVAL,
            empty_ticks: 0,
            last_server_status: STATUS_INVALID,
            status_icon: None,
            random: mc_core::random::create_new(),
            tickables: Vec::new(),
            local_ip: String::new(),
            port: -1,
            online_mode: false,
            prevent_proxy_connections: false,
            is_demo: false,
            server_id: None,
            effective_respawn_data: None,
        };
        server.weather_data();
        server.init_clock_manager();
        Ok(server)
    }

    // ------------------------------------------------------------------ level container

    pub fn add_level(&mut self, level: ServerLevel) {
        self.levels.insert(level.dimension.clone(), Some(Box::new(level)));
    }

    pub fn level(&self, dimension: &DimensionKey) -> Option<&ServerLevel> {
        self.levels.get(dimension).and_then(|l| l.as_deref())
    }

    pub fn level_mut(&mut self, dimension: &DimensionKey) -> Option<&mut ServerLevel> {
        self.levels.get_mut(dimension).and_then(|l| l.as_deref_mut())
    }

    /// Dimension keys in vanilla order (overworld first).
    pub fn level_keys(&self) -> Vec<DimensionKey> {
        self.levels.keys().cloned().collect()
    }

    /// Take `dimension` out of the level map, run `f` with `&mut MinecraftServer` (that level absent: queries for it
    /// return `None`) and `&mut ServerLevel`, then put it back. This is how a level ticks while still being able to
    /// reach the server and the other levels. Returns `None` if the level doesn't exist or is already taken out.
    /// The level is put back even when `f` panics.
    pub fn with_level<R>(&mut self, dimension: &DimensionKey, f: impl FnOnce(&mut MinecraftServer, &mut ServerLevel) -> R) -> Option<R> {
        let mut level = self.levels.get_mut(dimension)?.take()?;
        let result = panic::catch_unwind(AssertUnwindSafe(|| f(self, &mut level)));
        if let Some(slot) = self.levels.get_mut(dimension) {
            *slot = Some(level);
        }
        match result {
            Ok(r) => Some(r),
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    /// All levels that are currently in place (`getAllLevels`).
    pub fn all_levels(&self) -> impl Iterator<Item = &ServerLevel> {
        self.levels.values().filter_map(|l| l.as_deref())
    }

    pub fn all_levels_mut(&mut self) -> impl Iterator<Item = &mut ServerLevel> {
        self.levels.values_mut().filter_map(|l| l.as_deref_mut())
    }

    /// `overworld()`.
    pub fn overworld(&self) -> &ServerLevel {
        self.level(&super::world_dimensions::overworld_key()).expect("overworld is not loaded (or taken out for its own tick)")
    }

    pub fn overworld_mut(&mut self) -> &mut ServerLevel {
        self.level_mut(&super::world_dimensions::overworld_key()).expect("overworld is not loaded (or taken out for its own tick)")
    }

    // ------------------------------------------------------------------ game rules

    /// `getGameRules()`: the rules live in the root saved data (`data/minecraft/game_rules.dat`).
    pub fn game_rules(&mut self) -> &mut GameRules {
        self.saved_data.compute_if_absent(&self.game_rules_type)
    }

    /// `getGameRules().get(rule)`.
    pub fn game_rule<T: GameRuleValueType>(&mut self, rule: crate::world::level::gamerules::GameRule<T>) -> T {
        self.game_rules().get(rule)
    }

    /// `getGameRules().set(rule, value, this)`: change a rule and run [`Self::on_game_rule_changed`].
    pub fn set_game_rule<T: GameRuleValueType>(&mut self, rule: crate::world::level::gamerules::GameRule<T>, value: T) {
        let value = value.into_value();
        if self.game_rules().try_set_value(rule.erase(), value) {
            self.on_game_rule_changed(rule.erase(), value);
        }
    }

    // ------------------------------------------------------------------ task queue (`BlockableEventLoop`)

    /// `runningTask()`: inside a task.
    pub fn running_task(&self) -> bool {
        self.tasks.reentrant_count != 0
    }

    /// `shouldRunAllTasks()`: inside `managedBlock`.
    fn should_run_all_tasks(&self) -> bool {
        self.tasks.blocking_count > 0
    }

    /// `scheduleExecutables()`: on the game thread a task is only deferred while another one runs (reentrancy) and
    /// the server is not stopped.
    fn schedule_executables(&self) -> bool {
        self.running_task() && !self.shared.is_stopped()
    }

    /// `MinecraftServer.execute(task)` on the game thread: run inline unless called from inside a task (then queued
    /// like `ServerShared::execute`).
    pub fn execute(&mut self, task: impl FnOnce(&mut MinecraftServer) + Send + 'static) {
        if self.schedule_executables() {
            self.shared.execute(task);
        } else {
            self.do_run_task(TickTask { tick: self.tick_count, run: Box::new(task) });
        }
    }

    /// `executeIfPossible`: rejected once the server is stopped.
    pub fn execute_if_possible(&mut self, task: impl FnOnce(&mut MinecraftServer) + Send + 'static) -> Result<(), &'static str> {
        if self.shared.is_stopped() {
            return Err("Server already shutting down");
        }
        self.execute(task);
        Ok(())
    }

    /// `executeBlocking` on the game thread: runs inline.
    pub fn execute_blocking<R>(&mut self, task: impl FnOnce(&mut MinecraftServer) -> R) -> R {
        task(self)
    }

    /// `getPendingTasksCount`.
    pub fn pending_tasks_count(&mut self) -> usize {
        self.tasks.len()
    }

    /// `runAllTasks`.
    pub fn run_all_tasks(&mut self) {
        while self.poll_task() {}
    }

    /// `doRunTask`: a panicking task is logged and the loop carries on (vanilla catches `Exception` there).
    fn do_run_task(&mut self, task: TickTask) {
        self.tasks.reentrant_count += 1;
        self.shared.set_running_task(true);
        let result = panic::catch_unwind(AssertUnwindSafe(|| (task.run)(self)));
        self.tasks.reentrant_count -= 1;
        if self.tasks.reentrant_count == 0 {
            self.shared.set_running_task(false);
        }
        if let Err(payload) = result {
            let text = crate::crash_report::exception_text(payload.as_ref());
            log::error!("Error executing task on Server\n{text}");
        }
    }

    /// `shouldRun(task)`: late tasks run regardless of the tick budget.
    fn should_run(&self, task: &TickTask) -> bool {
        task.tick.wrapping_add(MAX_TICK_LATENCY) < self.tick_count || self.have_time()
    }

    /// `BlockableEventLoop.pollTask`: throw a delayed crash, then run the head task if allowed.
    fn poll_queued_task(&mut self) -> bool {
        if let Some(report) = self.shared.take_delayed_crash() {
            ReportedException::throw(report);
        }
        let (head_tick, has_head) = match self.tasks.peek() {
            Some(head) => (head.tick, true),
            None => (0, false),
        };
        if !has_head {
            return false;
        }
        let run = self.should_run_all_tasks() || head_tick.wrapping_add(MAX_TICK_LATENCY) < self.tick_count || self.have_time();
        if !run {
            return false;
        }
        let task = self.tasks.pop().expect("peeked task");
        self.do_run_task(task);
        true
    }

    /// `MinecraftServer.pollTask`: queued tasks first, then (when there is time) the chunk sources' main-thread work.
    pub fn poll_task(&mut self) -> bool {
        let may_have_more = self.poll_task_internal();
        self.may_have_delayed_tasks = may_have_more;
        self.sync_have_time();
        may_have_more
    }

    fn poll_task_internal(&mut self) -> bool {
        if self.poll_queued_task() {
            return true;
        }
        if self.tick_rate_manager.is_sprinting() || self.should_run_all_tasks() || self.have_time() {
            for level in self.all_levels_mut() {
                if level.poll_chunk_task() {
                    return true;
                }
            }
        }
        false
    }

    /// `managedBlock`: run tasks until `condition` holds.
    pub fn managed_block(&mut self, mut condition: impl FnMut(&mut MinecraftServer) -> bool) {
        self.tasks.blocking_count += 1;
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            while !condition(self) {
                if !self.poll_task() {
                    self.wait_for_tasks();
                }
            }
        }));
        self.tasks.blocking_count -= 1;
        if let Err(payload) = result {
            panic::resume_unwind(payload);
        }
    }

    /// `MinecraftServer.waitForTasks`: park until the next tick is due (while waiting for it) or 100 µs.
    fn wait_for_tasks(&mut self) {
        let wait_nanos = if self.waiting_for_next_tick { self.next_tick_time_nanos - get_nanos() } else { 100_000 };
        if wait_nanos > 0 {
            std::thread::park_timeout(Duration::from_nanos(wait_nanos as u64));
        }
    }

    /// `haveTime()`.
    pub fn have_time(&self) -> bool {
        self.shared.have_time()
    }

    /// Mirror `haveTime`'s inputs into `ServerShared` (they change at the same places as in vanilla).
    fn sync_have_time(&self) {
        self.shared.set_next_tick_time_nanos(self.next_tick_time_nanos);
        self.shared.set_have_time_deadline(if self.may_have_delayed_tasks { self.delayed_tasks_max_next_tick_time_nanos } else { self.next_tick_time_nanos });
    }

    pub(super) fn set_next_tick_time(&mut self, nanos: i64) {
        self.next_tick_time_nanos = nanos;
        self.sync_have_time();
    }

    /// `getNextTickTime`.
    pub fn next_tick_time(&self) -> i64 {
        self.next_tick_time_nanos
    }

    /// `waitUntilNextTick`.
    pub fn wait_until_next_tick(&mut self) {
        self.run_all_tasks();
        self.waiting_for_next_tick = true;
        self.managed_block(|server| !server.have_time());
        self.waiting_for_next_tick = false;
    }

    // ------------------------------------------------------------------ run loop

    /// `runServer`: `initServer`, the tick loop, crash handling, `stopServer`. Call on the game thread.
    pub fn run_server(&mut self) {
        let result = panic::catch_unwind(AssertUnwindSafe(|| self.run_loop()));
        if let Err(payload) = result {
            let report = self.crash_report_for(payload);
            log::error!("Encountered an unexpected exception\n{}", report.exception());
            self.finish_crash_report(report);
        }
        self.shared.set_stopped();
        if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| self.stop_server())) {
            log::error!("Exception stopping the server\n{}", crate::crash_report::exception_text(payload.as_ref()));
        }
        self.on_server_exit();
        self.shared.set_exited();
    }

    fn run_loop(&mut self) {
        match self.init_server() {
            Ok(true) => {}
            Ok(false) => panic!("Failed to initialize server"),
            Err(e) => panic!("{e}"),
        }
        self.set_next_tick_time(get_nanos());
        let world_icon = self.storage_source.get_icon_file();
        self.status_icon = load_status_icon(&self.get_file("server-icon.png"), world_icon);
        self.refresh_status();

        while self.shared.is_running() {
            let this_tick_nanos;
            if !self.is_paused() && self.tick_rate_manager.is_sprinting() && self.check_should_sprint_this_tick() {
                this_tick_nanos = 0;
                self.next_tick_time_nanos = get_nanos();
                self.last_overload_warning_nanos = self.next_tick_time_nanos;
            } else {
                this_tick_nanos = self.tick_rate_manager.nanoseconds_per_tick();
                let overload = tick_pacing::check_overloaded(
                    get_nanos(),
                    &mut self.next_tick_time_nanos,
                    &mut self.last_overload_warning_nanos,
                    this_tick_nanos,
                );
                if let Some(overload) = overload {
                    log::warn!("Can't keep up! Is the server overloaded? Running {}ms or {} ticks behind", overload.millis, overload.ticks);
                }
            }

            let sprinting = this_tick_nanos == 0;
            let next = self.next_tick_time_nanos + this_tick_nanos;
            self.set_next_tick_time(next);

            self.process_packets_and_tick(sprinting);
            self.may_have_delayed_tasks = true;
            self.delayed_tasks_max_next_tick_time_nanos = (get_nanos() + this_tick_nanos).max(self.next_tick_time_nanos);
            self.sync_have_time();
            self.wait_until_next_tick();
            if sprinting {
                self.tick_rate_manager.end_tick_work();
            }
            self.last_tick_nanos = get_nanos();

            self.shared.set_ready();
        }
    }

    /// `constructOrExtractCrashReport`: the panic's own report when it is a `ReportedException`, else a new one
    /// titled "Exception in server tick loop"; then `fillSystemReport`.
    fn crash_report_for(&mut self, payload: Box<dyn std::any::Any + Send>) -> CrashReport {
        let mut report = CrashReport::for_throwable(payload, "Exception in server tick loop");
        self.fill_system_report(report.system_report());
        report
    }

    /// The tail of the `catch (Throwable)` block of `runServer`: save the report, then `onServerCrash`.
    fn finish_crash_report(&mut self, mut report: CrashReport) {
        let file = self.server_directory.join("crash-reports").join(format!("crash-{}-server.txt", crate::crash_report::filename_formatted_date_time()));
        if report.save_to_file(&file) {
            log::error!("This crash report has been saved to: {}", std::path::absolute(&file).unwrap_or(file).display());
        } else {
            log::error!("We were unable to save this crash report to disk.");
        }
        self.on_server_crash(&report);
    }

    pub fn is_paused(&self) -> bool {
        false
    }

    /// `getFile(name)` / `getServerDirectory().resolve(name)`.
    pub fn get_file(&self, name: &str) -> PathBuf {
        self.server_directory.join(name)
    }

    pub fn get_server_directory(&self) -> &Path {
        &self.server_directory
    }

    // ------------------------------------------------------------------ one tick

    /// `processPacketsAndTick`.
    fn process_packets_and_tick(&mut self, sprinting: bool) {
        // `packetProcessor.processQueuedPackets()`: queued packets are handled by the network layer.
        self.tick_server(sprinting);
    }

    /// `tickServer`. `sprinting`: the level tick gets `() -> false` as time budget.
    pub fn tick_server(&mut self, sprinting: bool) {
        let nano = get_nanos();
        let empty_tick_threshold = self.pause_when_empty_seconds().wrapping_mul(20);
        if empty_tick_threshold > 0 {
            if self.player_list.get_player_count() == 0 && !self.tick_rate_manager.is_sprinting() {
                self.empty_ticks += 1;
            } else {
                self.empty_ticks = 0;
            }
            if self.empty_ticks >= empty_tick_threshold {
                if self.empty_ticks == empty_tick_threshold {
                    log::info!("Server empty for {} seconds, pausing", self.pause_when_empty_seconds());
                    self.auto_save();
                }
                self.tick_connection();
                self.refresh_shared_info();
                return;
            }
        }

        self.tick_count = self.tick_count.wrapping_add(1);
        self.shared.set_tick_count(self.tick_count);
        self.tick_rate_manager.tick();
        if sprinting {
            self.tick_children(&mut || false);
        } else {
            let shared = self.shared.clone();
            self.tick_children(&mut move || shared.have_time());
        }
        if nano - self.last_server_status >= tick_pacing::STATUS_EXPIRE_TIME_NANOS {
            self.last_server_status = nano;
            self.refresh_status();
        }

        self.ticks_until_autosave -= 1;
        if self.ticks_until_autosave <= 0 {
            self.auto_save();
        }

        let tick_time = get_nanos() - nano;
        self.tick_times.record(self.tick_count, tick_time);
        self.refresh_shared_info();
    }

    /// `autoSave`.
    fn auto_save(&mut self) {
        self.ticks_until_autosave = self.compute_next_autosave_interval();
        log::debug!("Autosave started");
        self.save_everything(true, false, false);
        log::debug!("Autosave finished");
    }

    fn compute_next_autosave_interval(&self) -> i32 {
        let ticks_per_second = if self.tick_rate_manager.is_sprinting() {
            tick_pacing::sprint_ticks_per_second(self.get_average_tick_time_nanos())
        } else {
            self.tick_rate_manager.tickrate()
        };
        tick_pacing::compute_next_autosave_interval(ticks_per_second)
    }

    /// `onTickRateChanged`.
    pub fn on_tick_rate_changed(&mut self) {
        let new_autosave_interval = self.compute_next_autosave_interval();
        if new_autosave_interval < self.ticks_until_autosave {
            self.ticks_until_autosave = new_autosave_interval;
        }
    }

    /// `addTickable`.
    pub fn add_tickable(&mut self, tickable: impl FnMut() + Send + 'static) {
        self.tickables.push(Box::new(tickable));
    }

    pub(super) fn run_tickables(&mut self) {
        for tickable in &mut self.tickables {
            tickable();
        }
    }

    pub fn get_current_smoothed_tick_time(&self) -> f32 {
        self.tick_times.smoothed_tick_time_millis()
    }

    pub fn get_average_tick_time_nanos(&self) -> i64 {
        self.tick_times.average_tick_time_nanos(self.tick_count)
    }

    pub fn get_tick_times_nanos(&self) -> &[i64; tick_pacing::TICK_STATS_SPAN] {
        self.tick_times.tick_times_nanos()
    }

    // ------------------------------------------------------------------ status

    /// `invalidateStatus`.
    pub fn invalidate_status(&mut self) {
        self.last_server_status = STATUS_INVALID;
    }

    /// `status = buildServerStatus()`, published to the network threads.
    fn refresh_status(&mut self) {
        let status = self.build_server_status();
        self.shared.set_status(status);
    }

    pub(super) fn status_icon(&self) -> Option<&[u8]> {
        self.status_icon.as_deref()
    }

    pub(super) fn status_random(&mut self) -> &mut LegacyRandomSource {
        &mut self.random
    }

    /// `fillSystemReport`: the common part (`MinecraftServer`), then `fillServerSystemReport` (dedicated).
    pub fn fill_system_report(&self, report: &mut SystemReport) {
        report.set_detail("Server Running", self.shared.is_running());
        report.set_detail(
            "Player Count",
            format!("{} / {}; {}", self.player_list.get_player_count(), self.player_list.get_max_players(), player_names_debug(&self.player_list)),
        );
        report.set_detail("Active Data Packs", self.pack_repository.selected_ids().join(", "));
        report.set_detail("Enabled Feature Flags", format!("{:?}", self.world_data.lock().settings.data_configuration.enabled_features));
        report.set_detail("World Seed", self.world_gen_settings.options.seed);
        self.fill_system_report_details(report);
    }

    /// `getSelectedPacks`-style data configuration is persisted by `world_data`; `GameRuleValue` helpers live in
    /// `on_game_rule_changed`.
    pub(super) fn game_rule_value_bool(value: GameRuleValue) -> bool {
        matches!(value, GameRuleValue::Bool(true))
    }
}

fn player_names_debug(list: &PlayerList) -> String {
    format!("[{}]", list.get_players().iter().map(|p| p.name().to_owned()).collect::<Vec<_>>().join(", "))
}
