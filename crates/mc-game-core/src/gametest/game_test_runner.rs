//! `GameTestRunner`: runs batches of game tests one after another, each in its test environment.

use super::game_test_batch::{GameTestBatch, GameTestBatchListener, GameTestBatcher, from_game_test_info};
use super::game_test_info::GameTestInfo;
use super::game_test_listener::GameTestListener;
use super::game_test_ticker;
use super::multiple_test_tracker::MultipleTestTracker;
use super::report_game_listener::ReportGameListener;
use super::structure_utils;
use super::test_environment_definition::Activation;
use crate::server::MinecraftServer;
use crate::world::level::LevelAccessor;
use mc_core::pos::ChunkPos;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// `DEFAULT_TESTS_PER_ROW`.
pub const DEFAULT_TESTS_PER_ROW: i32 = 8;

/// `GameTestRunner.StructureSpawner`.
pub trait StructureSpawner {
    fn spawn_structure(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>) -> Option<Rc<GameTestInfo>>;

    fn on_batch_start(&self, _server: &mut MinecraftServer) {}
}

/// `StructureSpawner.IN_PLACE`: the structure is placed at the test's block (`prepareTestStructure().startExecution(1)`).
pub struct InPlace;

impl StructureSpawner for InPlace {
    fn spawn_structure(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>) -> Option<Rc<GameTestInfo>> {
        test_info.prepare_test_structure(server).map(|info| info.start_execution(1))
    }
}

/// `StructureSpawner.NOT_SET`.
pub struct NotSet;

impl StructureSpawner for NotSet {
    fn spawn_structure(&self, _server: &mut MinecraftServer, _test_info: &Rc<GameTestInfo>) -> Option<Rc<GameTestInfo>> {
        None
    }
}

pub struct GameTestRunner {
    me: Weak<GameTestRunner>,
    all_test_infos: RefCell<Vec<Rc<GameTestInfo>>>,
    batches: RefCell<Vec<Rc<GameTestBatch>>>,
    batch_listeners: RefCell<Vec<Rc<dyn GameTestBatchListener>>>,
    scheduled_for_rerun: RefCell<Vec<Rc<GameTestInfo>>>,
    test_batcher: GameTestBatcher,
    stopped: Cell<bool>,
    current_environment: RefCell<Option<Activation>>,
    existing_structure_spawner: Rc<dyn StructureSpawner>,
    new_structure_spawner: Rc<dyn StructureSpawner>,
    halt_on_error: bool,
    clear_between_batches: bool,
}

impl GameTestRunner {
    /// `getTestInfos()` (a snapshot; reruns are appended while running).
    pub fn get_test_infos(&self) -> Vec<Rc<GameTestInfo>> {
        self.all_test_infos.borrow().clone()
    }

    fn rc(&self) -> Rc<GameTestRunner> {
        self.me.upgrade().expect("the runner is alive while it runs")
    }

    /// `start()`.
    pub fn start(&self, server: &mut MinecraftServer) {
        self.stopped.set(false);
        self.run_batch(server, 0);
    }

    /// `stop()`.
    pub fn stop(&self, server: &mut MinecraftServer) {
        self.stopped.set(true);
        self.end_current_environment(server);
    }

    /// `rerunTest(info)`.
    pub fn rerun_test(&self, server: &mut MinecraftServer, info: &Rc<GameTestInfo>) {
        let runner = self.rc();
        let copy = info.copy_reset();
        for listener in info.get_listeners() {
            listener.test_added_for_rerun(server, &listener, info, &copy, &runner);
        }
        self.all_test_infos.borrow_mut().push(copy.clone());
        self.scheduled_for_rerun.borrow_mut().push(copy);
        if self.stopped.get() {
            self.run_scheduled_rerun_tests(server);
        }
    }

    fn run_batch(&self, server: &mut MinecraftServer, batch_index: usize) {
        let batches = self.batches.borrow().clone();
        if batch_index >= batches.len() {
            self.end_current_environment(server);
            self.run_scheduled_rerun_tests(server);
            return;
        }
        if batch_index > 0 && self.clear_between_batches {
            for info in &batches[batch_index - 1].game_test_infos {
                if let Some(view) = info.get_test_instance_view(server)
                    && let Some(level) = server.level_mut(info.level())
                {
                    structure_utils::clear_space_for_structure(level, &view.get_test_bounding_box());
                    level.destroy_block(view.pos, false, None, 512);
                }
            }
        }
        let current_batch = batches[batch_index].clone();
        self.existing_structure_spawner.on_batch_start(server);
        self.new_structure_spawner.on_batch_start(server);
        let test_infos_for_this_batch: Vec<Rc<GameTestInfo>> = current_batch.game_test_infos.iter().filter_map(|info| self.spawn(server, info)).collect();
        log::info!(
            "Running test environment '{}' batch {} ({} tests)...",
            current_batch.environment.registered_name(),
            current_batch.index,
            test_infos_for_this_batch.len()
        );
        self.end_current_environment(server);
        let registries = server.registries.composite_access().clone();
        if let Some(definition) = current_batch.environment.value(&registries) {
            *self.current_environment.borrow_mut() = Some(Activation::activate(server, definition, &current_batch.dimension));
        }
        for listener in self.batch_listeners.borrow().clone() {
            listener.test_batch_starting(server, &current_batch);
        }
        let current_batch_tracker = MultipleTestTracker::new();
        for info in &test_infos_for_this_batch {
            current_batch_tracker.add_test_to_track(info.clone());
        }
        current_batch_tracker.add_listener(Rc::new(BatchListener {
            runner: self.me.clone(),
            tracker: current_batch_tracker.clone(),
            batch: current_batch.clone(),
            batch_index,
        }));
        for info in test_infos_for_this_batch {
            game_test_ticker::add(info);
        }
    }

    fn end_current_environment(&self, server: &mut MinecraftServer) {
        let activation = self.current_environment.borrow_mut().take();
        if let Some(activation) = activation {
            activation.teardown(server);
        }
    }

    fn run_scheduled_rerun_tests(&self, server: &mut MinecraftServer) {
        let scheduled = std::mem::take(&mut *self.scheduled_for_rerun.borrow_mut());
        if !scheduled.is_empty() {
            log::info!("Starting re-run of tests: {}", scheduled.iter().map(|info| info.id().to_string()).collect::<Vec<_>>().join(", "));
            let batches = (self.test_batcher)(&scheduled);
            *self.batches.borrow_mut() = batches.into_iter().map(Rc::new).collect();
            self.stopped.set(false);
            self.run_batch(server, 0);
        } else {
            self.batches.borrow_mut().clear();
            self.stopped.set(true);
        }
    }

    /// `addListener(listener)`.
    pub fn add_listener(&self, listener: Rc<dyn GameTestBatchListener>) {
        self.batch_listeners.borrow_mut().push(listener);
    }

    /// `spawn(testInfo)`.
    fn spawn(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>) -> Option<Rc<GameTestInfo>> {
        if test_info.get_test_block_pos().is_none() {
            self.new_structure_spawner.spawn_structure(server, test_info)
        } else {
            self.existing_structure_spawner.spawn_structure(server, test_info)
        }
    }

    /// `testInfo.getLevel().getForceLoadedChunks().forEach(pos -> setChunkForced(x, z, false))`.
    fn unforce_chunks(server: &mut MinecraftServer, test_info: &GameTestInfo) {
        if let Some(level) = server.level_mut(test_info.level()) {
            let forced: Vec<i64> = level.chunk_source.get_force_loaded_chunks().iter().collect();
            for pos in forced {
                level.chunk_source.update_chunk_forced(ChunkPos::new(ChunkPos::get_x(pos), ChunkPos::get_z(pos)), false);
            }
        }
    }
}

/// The listener `runBatch` installs on the batch's tests (an anonymous `GameTestListener`).
struct BatchListener {
    runner: Weak<GameTestRunner>,
    tracker: Rc<MultipleTestTracker>,
    batch: Rc<GameTestBatch>,
    batch_index: usize,
}

impl BatchListener {
    fn test_completed(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>) {
        if self.tracker.is_done() {
            let Some(runner) = self.runner.upgrade() else { return };
            for listener in runner.batch_listeners.borrow().clone() {
                listener.test_batch_finished(server, &self.batch);
            }
            GameTestRunner::unforce_chunks(server, test_info);
            // the tracker and this listener reference each other through the tests
            self.tracker.release();
            runner.run_batch(server, self.batch_index + 1);
        }
    }
}

impl GameTestListener for BatchListener {
    fn test_structure_loaded(&self, _server: &mut MinecraftServer, _test_info: &Rc<GameTestInfo>) {}

    fn test_passed(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, _runner: &Rc<GameTestRunner>) {
        if let Some(pos) = test_info.get_test_block_pos() {
            crate::world::block_entity::test_instance_block_entity::remove_barriers(server, test_info.level(), pos);
        }
        self.test_completed(server, test_info);
    }

    fn test_failed(&self, server: &mut MinecraftServer, test_info: &Rc<GameTestInfo>, _runner: &Rc<GameTestRunner>) {
        let Some(runner) = self.runner.upgrade() else { return };
        if runner.halt_on_error {
            runner.end_current_environment(server);
            GameTestRunner::unforce_chunks(server, test_info);
            game_test_ticker::clear(server);
        } else {
            self.test_completed(server, test_info);
        }
    }

    fn test_added_for_rerun(
        &self,
        _server: &mut MinecraftServer,
        _listener: &Rc<dyn GameTestListener>,
        _original: &Rc<GameTestInfo>,
        _copy: &Rc<GameTestInfo>,
        _runner: &Rc<GameTestRunner>,
    ) {
    }
}

/// `GameTestRunner.Builder`.
pub struct Builder {
    batcher: GameTestBatcher,
    existing_structure_spawner: Rc<dyn StructureSpawner>,
    new_structure_spawner: Rc<dyn StructureSpawner>,
    batches: Vec<GameTestBatch>,
    halt_on_error: bool,
    clear_between_batches: bool,
}

impl Builder {
    /// `fromBatches(batches, server)`.
    pub fn from_batches(batches: Vec<GameTestBatch>) -> Builder {
        Builder {
            batcher: from_game_test_info(),
            existing_structure_spawner: Rc::new(InPlace),
            new_structure_spawner: Rc::new(NotSet),
            batches,
            halt_on_error: false,
            clear_between_batches: false,
        }
    }

    /// `fromInfo(tests, server)`.
    pub fn from_info(tests: Vec<Rc<GameTestInfo>>) -> Builder {
        Builder::from_batches(from_game_test_info()(&tests))
    }

    /// `haltOnError()`.
    pub fn halt_on_error(mut self) -> Builder {
        self.halt_on_error = true;
        self
    }

    /// `clearBetweenBatches()`.
    pub fn clear_between_batches(mut self) -> Builder {
        self.clear_between_batches = true;
        self
    }

    /// `newStructureSpawner(structureSpawner)`.
    pub fn new_structure_spawner(mut self, spawner: Rc<dyn StructureSpawner>) -> Builder {
        self.new_structure_spawner = spawner;
        self
    }

    /// `existingStructureSpawner(spawner)`.
    pub fn existing_structure_spawner(mut self, spawner: Rc<dyn StructureSpawner>) -> Builder {
        self.existing_structure_spawner = spawner;
        self
    }

    /// `batcher(batcher)`.
    pub fn batcher(mut self, batcher: GameTestBatcher) -> Builder {
        self.batcher = batcher;
        self
    }

    /// `build()`: registers the runner with the ticker and adds a `ReportGameListener` to every test.
    pub fn build(self) -> Rc<GameTestRunner> {
        let batches: Vec<Rc<GameTestBatch>> = self.batches.into_iter().map(Rc::new).collect();
        let all_test_infos: Vec<Rc<GameTestInfo>> = batches.iter().flat_map(|b| b.game_test_infos.iter().cloned()).collect();
        let runner = Rc::new_cyclic(|me| GameTestRunner {
            me: me.clone(),
            all_test_infos: RefCell::new(all_test_infos.clone()),
            batches: RefCell::new(batches),
            batch_listeners: RefCell::new(Vec::new()),
            scheduled_for_rerun: RefCell::new(Vec::new()),
            test_batcher: self.batcher,
            stopped: Cell::new(true),
            current_environment: RefCell::new(None),
            existing_structure_spawner: self.existing_structure_spawner,
            new_structure_spawner: self.new_structure_spawner,
            halt_on_error: self.halt_on_error,
            clear_between_batches: self.clear_between_batches,
        });
        game_test_ticker::set_runner(runner.clone());
        for info in &all_test_infos {
            info.add_listener(Rc::new(ReportGameListener::default()));
        }
        runner
    }
}
