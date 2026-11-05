//! `ServerShared`: the thread-safe part of the server, handed out as `Arc<ServerShared>` to network tasks, chunk
//! worker threads and (through `ServerLevel::shared()`) to level code. Everything else about the server is owned by
//! the game thread (`MinecraftServer`) as plain `&mut` data. Add fields here only when something off the game thread
//! (or code that cannot reach `&mut MinecraftServer`) must read them; they must be `Send + Sync` (atomics, locks,
//! immutable data).

use super::MinecraftServer;
use super::server_info::{ServerInfo, ServerInterface};
use crate::crash_report::CrashReport;
use mc_protocol::packets::status::ServerStatus;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, LazyLock, OnceLock};
use std::thread::{self, Thread, ThreadId};
use std::time::Instant;

/// A unit of work run on the game thread between ticks (`MinecraftServer.execute`/`executeIfPossible`).
pub type ServerTask = Box<dyn FnOnce(&mut MinecraftServer) + Send>;

/// `TickTask`: a task stamped with the tick it was submitted in (`shouldRun` runs it late tasks regardless of the
/// tick budget: `tick + 3 < tickCount`).
pub struct TickTask {
    pub tick: i32,
    pub run: ServerTask,
}

/// `Util.getNanos()` (`System.nanoTime()`): monotonic nanoseconds from an arbitrary origin.
pub fn get_nanos() -> i64 {
    static ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);
    ORIGIN.elapsed().as_nanos() as i64
}

/// Values mirrored from the game thread for readers off it (`ServerInfo`/`ServerInterface` consumers: query, rcon,
/// management API). Refreshed by `MinecraftServer::refresh_shared_info` every tick.
#[derive(Default, Clone)]
pub struct ServerInfoSnapshot {
    pub motd: String,
    pub server_version: String,
    pub player_count: i32,
    pub max_players: i32,
    pub player_names: Vec<String>,
    pub server_ip: String,
    pub server_port: i32,
    pub level_id_name: String,
}

pub struct ServerShared {
    /// `MinecraftServer.running` (cleared by `halt`; polled by the tick loop).
    running: AtomicBool,
    /// `MinecraftServer.stopped`: set once the run loop ended and `stopServer` is about to run.
    stopped: AtomicBool,
    /// `MinecraftServer.isReady`: the first full tick finished.
    ready: AtomicBool,
    /// `MinecraftServer.isSaving`.
    saving: AtomicBool,
    /// The game thread ran to completion (`isShutdown`).
    exited: AtomicBool,
    /// `MinecraftServer.tickCount`, mirrored for readers off the game thread (status, watchdog).
    tick_count: AtomicI32,
    /// `MinecraftServer.nextTickTimeNanos` (read by the watchdog).
    next_tick_time_nanos: AtomicI64,
    /// `haveTime()` = `runningTask() || nanos < deadline`, with `deadline = mayHaveDelayedTasks ?
    /// delayedTasksMaxNextTickTimeNanos : nextTickTimeNanos`.
    have_time_deadline_nanos: AtomicI64,
    running_task: AtomicBool,
    task_tx: Sender<TickTask>,
    game_thread: OnceLock<Thread>,
    /// `MinecraftServer.status`, served to status pings.
    status: RwLock<Option<Arc<ServerStatus>>>,
    info: RwLock<ServerInfoSnapshot>,
    /// `BlockableEventLoop.delayedCrash`: a crash reported by another thread, thrown by the next `pollTask`.
    delayed_crash: Mutex<Option<CrashReport>>,
    /// `MinecraftServer.getRecipeManager()`: swapped by `/reload` (see `MinecraftServer::publish_recipe_manager`).
    recipe_manager: RwLock<crate::world::item::crafting::RecipeManager>,
}

impl ServerShared {
    pub fn new() -> (Arc<Self>, Receiver<TickTask>) {
        let (tx, rx) = channel();
        let shared = Arc::new(Self {
            running: AtomicBool::new(true),
            stopped: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            saving: AtomicBool::new(false),
            exited: AtomicBool::new(false),
            tick_count: AtomicI32::new(0),
            next_tick_time_nanos: AtomicI64::new(get_nanos()),
            have_time_deadline_nanos: AtomicI64::new(i64::MIN),
            running_task: AtomicBool::new(false),
            task_tx: tx,
            game_thread: OnceLock::new(),
            status: RwLock::new(None),
            info: RwLock::new(ServerInfoSnapshot::default()),
            delayed_crash: Mutex::new(None),
            recipe_manager: RwLock::new(crate::world::item::crafting::RecipeManager::empty()),
        });
        (shared, rx)
    }

    /// `getRecipeManager()` (a cheap handle).
    pub fn recipe_manager(&self) -> crate::world::item::crafting::RecipeManager {
        self.recipe_manager.read().clone()
    }

    pub fn set_recipe_manager(&self, manager: crate::world::item::crafting::RecipeManager) {
        *self.recipe_manager.write() = manager;
    }

    /// Register the calling thread as the game thread (`serverThread`): `execute` unparks it, `is_game_thread`
    /// compares against it.
    pub fn bind_game_thread(&self) {
        let _ = self.game_thread.set(thread::current());
    }

    pub fn is_game_thread(&self) -> bool {
        self.game_thread.get().is_some_and(|t| t.id() == thread::current().id())
    }

    pub fn game_thread_id(&self) -> Option<ThreadId> {
        self.game_thread.get().map(Thread::id)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
    /// `halt(false)`: ask the server to stop after the current tick.
    pub fn halt(&self) {
        self.running.store(false, Ordering::Release);
        self.unpark_game_thread();
    }
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    pub(crate) fn set_stopped(&self) {
        self.stopped.store(true, Ordering::Release);
    }
    /// `isShutdown`: the game thread has finished.
    pub fn is_shutdown(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }
    pub(crate) fn set_exited(&self) {
        self.exited.store(true, Ordering::Release);
    }
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    pub(crate) fn set_ready(&self) {
        self.ready.store(true, Ordering::Release);
    }
    /// `isCurrentlySaving`.
    pub fn is_currently_saving(&self) -> bool {
        self.saving.load(Ordering::Acquire)
    }
    pub(crate) fn set_saving(&self, saving: bool) {
        self.saving.store(saving, Ordering::Release);
    }

    pub fn tick_count(&self) -> i32 {
        self.tick_count.load(Ordering::Relaxed)
    }
    pub(crate) fn set_tick_count(&self, n: i32) {
        self.tick_count.store(n, Ordering::Relaxed);
    }

    /// `getNextTickTime`.
    pub fn next_tick_time_nanos(&self) -> i64 {
        self.next_tick_time_nanos.load(Ordering::Relaxed)
    }
    pub(crate) fn set_next_tick_time_nanos(&self, nanos: i64) {
        self.next_tick_time_nanos.store(nanos, Ordering::Relaxed);
    }
    pub(crate) fn set_have_time_deadline(&self, nanos: i64) {
        self.have_time_deadline_nanos.store(nanos, Ordering::Relaxed);
    }
    /// `BlockableEventLoop.scheduleExecutables()` of the server for the calling thread: tasks submitted with `execute` are
    /// queued unless the game thread submits them while no task runs (or the server is stopped): those run inline.
    pub fn runs_tasks_inline(&self) -> bool {
        self.is_game_thread() && (!self.running_task.load(Ordering::Relaxed) || self.is_stopped())
    }

    pub(crate) fn set_running_task(&self, running: bool) {
        self.running_task.store(running, Ordering::Relaxed);
    }

    /// `MinecraftServer.haveTime()`: whether the current tick still has time budget (handed to level ticking and
    /// chunk-source ticking as `BooleanSupplier`).
    pub fn have_time(&self) -> bool {
        self.running_task.load(Ordering::Relaxed) || get_nanos() < self.have_time_deadline_nanos.load(Ordering::Relaxed)
    }

    /// `getStatus`: the status JSON model served to server-list pings (`None` until the first one is built).
    pub fn status(&self) -> Option<Arc<ServerStatus>> {
        self.status.read().clone()
    }
    pub(crate) fn set_status(&self, status: ServerStatus) {
        *self.status.write() = Some(Arc::new(status));
    }

    pub fn info(&self) -> ServerInfoSnapshot {
        self.info.read().clone()
    }
    pub(crate) fn set_info(&self, info: ServerInfoSnapshot) {
        *self.info.write() = info;
    }

    /// `BlockableEventLoop.relayDelayCrash`: report a crash from another thread; the game thread throws it from
    /// the next `pollTask`. The first report wins, later ones are dropped (vanilla only chains suppressed causes).
    pub fn relay_delay_crash(&self, report: CrashReport) {
        let mut slot = self.delayed_crash.lock();
        if slot.is_none() {
            *slot = Some(report);
        }
        drop(slot);
        self.unpark_game_thread();
    }
    pub(crate) fn take_delayed_crash(&self) -> Option<CrashReport> {
        self.delayed_crash.lock().take()
    }

    /// `MinecraftServer.execute(task)` from any thread (including the game thread, e.g. level code holding only
    /// `Arc<ServerShared>`): queue `task` for the game thread's next task drain.
    pub fn execute(&self, task: impl FnOnce(&mut MinecraftServer) + Send + 'static) {
        // A send error means the game thread is gone (server stopped): dropping the task matches vanilla.
        let _ = self.task_tx.send(TickTask { tick: self.tick_count(), run: Box::new(task) });
        self.unpark_game_thread();
    }

    /// `executeBlocking` from another thread: run `task` on the game thread and wait for its result. `None` when
    /// the game thread exited without running it. Panics when called on the game thread (it would deadlock;
    /// vanilla runs the task inline there, use `MinecraftServer::execute_blocking`).
    pub fn execute_blocking<R: Send + 'static>(&self, task: impl FnOnce(&mut MinecraftServer) -> R + Send + 'static) -> Option<R> {
        assert!(!self.is_game_thread(), "execute_blocking called on the game thread");
        let (tx, rx) = channel();
        self.execute(move |server| {
            let _ = tx.send(task(server));
        });
        rx.recv().ok()
    }

    fn unpark_game_thread(&self) {
        if let Some(thread) = self.game_thread.get() {
            thread.unpark();
        }
    }
}

impl ServerInfo for ServerShared {
    fn get_motd(&self) -> String {
        self.info.read().motd.clone()
    }
    fn get_server_version(&self) -> String {
        self.info.read().server_version.clone()
    }
    fn get_player_count(&self) -> i32 {
        self.info.read().player_count
    }
    fn get_max_players(&self) -> i32 {
        self.info.read().max_players
    }
}

impl ServerInterface for ServerShared {
    fn get_server_ip(&self) -> String {
        self.info.read().server_ip.clone()
    }
    fn get_server_port(&self) -> i32 {
        self.info.read().server_port
    }
    fn get_server_name(&self) -> String {
        self.get_motd()
    }
    fn get_player_names(&self) -> Vec<String> {
        self.info.read().player_names.clone()
    }
    fn get_level_id_name(&self) -> String {
        self.info.read().level_id_name.clone()
    }
    fn get_plugin_names(&self) -> String {
        String::new()
    }
}
