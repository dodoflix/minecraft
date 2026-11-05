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
use std::ptr::NonNull;
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
    /// `Services.profileResolver()`.
    pub profile_resolver: Arc<dyn crate::server::players::ProfileResolver>,
    pub level_load_listener: Box<dyn LevelLoadListener>,
}

/// Per-tick work owned by slices that have not landed (scheduled events need the function manager, raids need the raid
/// manager): they register a function here and `ServerLevel::tick` calls it at vanilla's position in the tick.
#[derive(Default, Clone, Copy)]
pub struct ServerTickHooks {
    /// `server.getScheduledEvents().tick(server, gameTime)` (inside `ServerLevel.tickTime`).
    pub tick_scheduled_events: Option<fn(&mut MinecraftServer, i64)>,
    /// `raids.tick(level)`.
    pub tick_raids: Option<fn(&mut MinecraftServer, &mut ServerLevel)>,
}

/// A level that is taken out of [`MinecraftServer::levels`] for its tick but reachable again through `level`/`level_mut`/
/// `with_level` while it is lent. Only the game thread ever sets or reads it.
struct LentLevel {
    dimension: DimensionKey,
    level: NonNull<ServerLevel>,
}

// SAFETY: set and read on the game thread only, and only for the duration of one `with_lent_level` call.
unsafe impl Send for LentLevel {}

pub struct MinecraftServer {
    pub shared: Arc<ServerShared>,
    /// Seams for per-tick work of unlanded slices, see [`ServerTickHooks`].
    pub tick_hooks: ServerTickHooks,
    tasks: TaskQueue,
    /// `levels` in vanilla iteration order. A level is `None` only while it is taken out for its own tick
    /// (see [`MinecraftServer::with_level`]).
    levels: IndexMap<DimensionKey, Option<Box<ServerLevel>>>,
    /// The level whose own tick is currently running, lent back to the server while commands run inside it
    /// (see [`MinecraftServer::with_lent_level`]).
    lent_level: Option<LentLevel>,
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
    /// `getCommandStorage()` (`CommandStorage` over `saved_data`: `/data ... storage`).
    pub command_storage: crate::world::level::saveddata::command_storage::CommandStorage,
    pub(super) game_rules_type: SavedDataType<GameRules>,
    pub(super) saved_types: SavedDataTypes,
    /// `randomSequences` shared with the levels' loot evaluation; copied into `saved_data` when saving.
    pub(super) loot_random_sequences: Arc<parking_lot::Mutex<crate::world::level::saveddata::random_sequences::RandomSequences>>,
    /// `metricsRecorder` & co: what `/perf` records.
    pub(super) metrics_recording: super::metrics_recording::MetricsRecording,
    pub registries: LayeredRegistryAccess<RegistryLayer>,
    pub resources: ReloadableResources,
    pub pack_repository: PackRepository,
    pub level_load_listener: Box<dyn LevelLoadListener>,
    pub tick_rate_manager: ServerTickRateManager,
    /// `getScoreboard()` (`ServerScoreboard`): loaded from `scoreboard.dat` by `create_levels`, stored by `save_all_chunks`.
    pub scoreboard: crate::server::level::SharedScoreboard,
    pub player_list: PlayerList,
    /// Players that left their level storage but still have a connection (`ServerPlayer.showEndCredits()` removes the
    /// player with `CHANGED_DIMENSION` and the poem plays until the respawn request), keyed by uuid.
    pub detached_players: std::collections::HashMap<uuid::Uuid, Box<crate::world::entity::ServerPlayer>>,
    pub notification_manager: Arc<NotificationManager>,
    pub name_to_id_cache: Arc<CachedUserNameToIdResolver>,
    /// `services().profileResolver()`.
    pub profile_resolver: Arc<dyn crate::server::players::ProfileResolver>,
    pub dedicated: DedicatedServer,
    /// `getCommands()` (`resources.managers.getCommands()` in vanilla).
    pub commands: Arc<crate::commands::Commands>,
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
        init.registries.composite_access().install_protocol_context();
        shared.bind_game_thread();
        let enabled_features = init.world_data.settings.data_configuration.enabled_features;
        let mut saved_data = SavedDataManager::new(mc_world::level_storage::SavedDataStorage::new(
            init.storage_source.get_level_path(mc_world::level_storage::LevelResource::DATA),
        ));
        let registries = init.registries;
        crate::server::players::server_join_host::set_send_commands(|server, player, permissions| {
            crate::commands::send_commands(server, player, &permissions.into());
        });
        let commands = Arc::new(crate::commands::Commands::new(
            crate::commands::CommandSelection::Dedicated,
            &crate::commands::CommandBuildContext::simple(
                Arc::new(crate::commands::command_build_context::AccessRegistries(registries.composite_access().clone())),
                mc_world::level_storage::FeatureFlagSet::VANILLA,
            ),
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
        if let Some(recipes) = init.resources.managers.listener::<crate::world::item::crafting::RecipeManager>() {
            shared.set_recipe_manager(recipes.clone());
        }
        let mut server = Self {
            tick_hooks: ServerTickHooks::default(),
            shared,
            tasks: TaskQueue::new(tasks),
            levels: IndexMap::new(),
            lent_level: None,
            tick_count: 0,
            server_directory: init.server_directory,
            storage_source: init.storage_source,
            player_data_storage: init.player_data_storage,
            world_data: Arc::new(Mutex::new(init.world_data)),
            world_gen_settings: init.world_gen_settings,
            world_gen_settings_dirty: true,
            saved_data,
            command_storage: Default::default(),
            game_rules_type,
            saved_types: SavedDataTypes::new(),
            loot_random_sequences: Default::default(),
            metrics_recording: Default::default(),
            registries,
            resources: init.resources,
            pack_repository: init.pack_repository,
            level_load_listener: init.level_load_listener,
            tick_rate_manager: ServerTickRateManager::new(),
            scoreboard: crate::server::level::SharedScoreboard::default(),
            commands,
            player_list,
            detached_players: std::collections::HashMap::new(),
            notification_manager: init.notification_manager,
            name_to_id_cache: init.name_to_id_cache,
            profile_resolver: init.profile_resolver,
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
        *server.loot_random_sequences.lock() = server.saved_data.compute_if_absent(&server.saved_types.random_sequences).clone();
        server.weather_data();
        server.saved_data.compute_if_absent(&server.saved_types.custom_boss_events);
        server.init_clock_manager();
        Ok(server)
    }

    // ------------------------------------------------------------------ level container

    pub fn add_level(&mut self, level: ServerLevel) {
        self.levels.insert(level.dimension.clone(), Some(Box::new(level)));
    }

    pub fn level(&self, dimension: &DimensionKey) -> Option<&ServerLevel> {
        match self.levels.get(dimension)? {
            Some(level) => Some(level),
            // SAFETY: `lent_level` is only set by `with_lent_level`, whose caller does not touch the level meanwhile.
            None => self.lent_level.as_ref().filter(|l| l.dimension == *dimension).map(|l| unsafe { l.level.as_ref() }),
        }
    }

    pub fn level_mut(&mut self, dimension: &DimensionKey) -> Option<&mut ServerLevel> {
        match self.levels.get_mut(dimension)? {
            Some(level) => Some(level),
            // SAFETY: see `level`; `&mut self` makes this the only access to the lent level.
            None => self.lent_level.as_mut().filter(|l| l.dimension == *dimension).map(|l| unsafe { l.level.as_mut() }),
        }
    }

    /// Runs `f` with `level` (which the caller took out of the server to tick it) reachable through
    /// `level`/`level_mut`/`with_level` again. Vanilla's `ServerLevel.tickTime` runs scheduled functions that act on their own
    /// level; the caller must not use `level` while `f` runs.
    pub(super) fn with_lent_level<R>(&mut self, level: &mut ServerLevel, f: impl FnOnce(&mut MinecraftServer) -> R) -> R {
        let lent = LentLevel { dimension: level.dimension.clone(), level: NonNull::from(&mut *level) };
        let previous = self.lent_level.replace(lent);
        let result = panic::catch_unwind(AssertUnwindSafe(|| f(self)));
        self.lent_level = previous;
        match result {
            Ok(r) => r,
            Err(payload) => panic::resume_unwind(payload),
        }
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
        let Some(mut level) = self.levels.get_mut(dimension)?.take() else {
            // The level of the running tick: lent for the call, taken out of the server while `f` has it.
            let lent = self.lent_level.take_if(|l| l.dimension == *dimension)?;
            // SAFETY: see `level_mut`; the lent pointer was taken out, so nothing else can reach the level during `f`.
            let result = panic::catch_unwind(AssertUnwindSafe(|| f(self, unsafe { &mut *lent.level.as_ptr() })));
            self.lent_level = Some(lent);
            return match result {
                Ok(r) => Some(r),
                Err(payload) => panic::resume_unwind(payload),
            };
        };
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
        let lent = self.lent_level.as_ref();
        // SAFETY: see `level`.
        self.levels.iter().filter_map(move |(key, l)| match l.as_deref() {
            Some(level) => Some(level),
            None => lent.filter(|l| l.dimension == *key).map(|l| unsafe { l.level.as_ref() }),
        })
    }

    pub fn all_levels_mut(&mut self) -> impl Iterator<Item = &mut ServerLevel> {
        let lent = self.lent_level.as_ref().map(|l| (l.dimension.clone(), l.level));
        self.levels.iter_mut().filter_map(move |(key, l)| match l.as_deref_mut() {
            Some(level) => Some(level),
            // SAFETY: see `level_mut`; no other access to the lent level exists while `&mut self` is borrowed.
            None => lent.as_ref().filter(|(d, _)| d == key).map(|(_, p)| unsafe { &mut *p.as_ptr() }),
        })
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

    /// [`Self::set_game_rule`] for a type-erased rule.
    pub fn set_game_rule_value(&mut self, rule: crate::world::level::gamerules::AnyGameRule, value: crate::world::level::gamerules::GameRuleValue) {
        if self.game_rules().try_set_value(rule, value) {
            self.on_game_rule_changed(rule, value);
        }
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

            let metrics_scope = self.begin_metrics_tick();
            self.process_packets_and_tick(sprinting);
            crate::util::profiling::push("nextTickWait");
            self.may_have_delayed_tasks = true;
            self.delayed_tasks_max_next_tick_time_nanos = (get_nanos() + this_tick_nanos).max(self.next_tick_time_nanos);
            self.sync_have_time();
            self.wait_until_next_tick();
            if sprinting {
                self.tick_rate_manager.end_tick_work();
            }
            crate::util::profiling::pop();
            self.last_tick_nanos = get_nanos();
            drop(metrics_scope);
            self.end_metrics_recording_tick();

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
        use crate::util::profiling::{pop, push};
        push("tick");
        push("scheduledPacketProcessing");
        // `packetProcessor.processQueuedPackets()`
        crate::server::network::game_listener::process_queued_packets(self);
        pop();
        self.tick_server(sprinting);
        pop();
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

        crate::util::profiling::push("tallying");
        let tick_time = get_nanos() - nano;
        self.tick_times.record(self.tick_count, tick_time);
        self.refresh_shared_info();
        crate::util::profiling::pop();
    }

    /// `autoSave`.
    fn auto_save(&mut self) {
        self.ticks_until_autosave = self.compute_next_autosave_interval();
        log::debug!("Autosave started");
        crate::util::profiling::push("save");
        self.save_everything(true, false, false);
        crate::util::profiling::pop();
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
