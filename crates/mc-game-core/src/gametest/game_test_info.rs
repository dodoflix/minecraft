//! `GameTestInfo`: one running (or finished) game test.

use super::game_test_exception::GameTestException;
use super::game_test_helper::GameTestHelper;
use super::game_test_instance::{GameTestInstance, TestHolder};
use super::game_test_listener::GameTestListener;
use super::game_test_runner::GameTestRunner;
use super::game_test_sequence::GameTestSequence;
use super::retry_options::RetryOptions;
use super::structure_utils;
use crate::server::MinecraftServer;
use crate::translatable;
use crate::world::block_entity::test_instance_block_entity::{
    self, Data, Status, TestInstanceView,
};
use crate::world::level::{DimensionKey, LevelAccessor};
use mc_core::ident::Identifier;
use mc_core::phys::AABB;
use mc_core::pos::{BlockPos, Vec3i};
use mc_core::rotation::Rotation;
use mc_data::block::Block;
use mc_text::Component;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// `Runnable` of the game test API: runs with a helper and may throw a `GameTestException`.
pub type Runnable = Rc<dyn Fn(&mut GameTestHelper<'_>) -> Result<(), GameTestException>>;

pub struct GameTestInfo {
    test: TestHolder,
    test_block_pos: Cell<Option<BlockPos>>,
    level: DimensionKey,
    listeners: RefCell<Vec<Rc<dyn GameTestListener>>>,
    timeout_ticks: i32,
    sequences: RefCell<Vec<Rc<GameTestSequence>>>,
    run_at_tick_time_map: RefCell<Vec<(i64, Runnable)>>,
    placed_structure: Cell<bool>,
    chunks_loaded: Cell<bool>,
    tick_count: Cell<i32>,
    started: Cell<bool>,
    retry_options: RetryOptions,
    /// `timer` (Guava `Stopwatch`): the running start and the elapsed time of earlier runs.
    timer_start: Cell<Option<Instant>>,
    timer_elapsed: Cell<Duration>,
    done: Cell<bool>,
    extra_rotation: Rotation,
    error: RefCell<Option<GameTestException>>,
    /// `testInstanceBlockEntity != null`.
    has_block_entity: Cell<bool>,
    /// `GameTestHelper.finalCheckAdded` (the helper is recreated per call here).
    pub(super) final_check_added: Cell<bool>,
}

impl GameTestInfo {
    pub fn new(
        test: TestHolder,
        extra_rotation: Rotation,
        level: DimensionKey,
        retry_options: RetryOptions,
    ) -> Rc<GameTestInfo> {
        let timeout_ticks = test.value.max_ticks();
        Rc::new(GameTestInfo {
            test,
            test_block_pos: Cell::new(None),
            level,
            listeners: RefCell::new(Vec::new()),
            timeout_ticks,
            sequences: RefCell::new(Vec::new()),
            run_at_tick_time_map: RefCell::new(Vec::new()),
            placed_structure: Cell::new(false),
            chunks_loaded: Cell::new(false),
            tick_count: Cell::new(0),
            started: Cell::new(false),
            retry_options,
            timer_start: Cell::new(None),
            timer_elapsed: Cell::new(Duration::ZERO),
            done: Cell::new(false),
            extra_rotation,
            error: RefCell::new(None),
            has_block_entity: Cell::new(false),
            final_check_added: Cell::new(false),
        })
    }

    /// `setTestBlockPos(pos)`.
    pub fn set_test_block_pos(&self, test_block_pos: Option<BlockPos>) {
        self.test_block_pos.set(test_block_pos);
    }

    /// `startExecution(tickDelay)`.
    pub fn start_execution(self: &Rc<Self>, tick_delay: i32) -> Rc<Self> {
        self.tick_count
            .set(-(self.test.value.setup_ticks() + tick_delay + 1));
        self.clone()
    }

    /// `placeStructure()`.
    pub fn place_structure(self: &Rc<Self>, server: &mut MinecraftServer) {
        if self.placed_structure.get() {
            return;
        }
        let Some(pos) = self.test_block_pos.get() else {
            return;
        };
        if !test_instance_block_entity::place_structure(server, &self.level, pos) {
            let name = TestInstanceView::read(server, &self.level, pos)
                .map(|v| v.get_test_name().get_string())
                .unwrap_or_default();
            self.fail(translatable!("test.error.structure.failure", name));
        }
        self.placed_structure.set(true);
        test_instance_block_entity::encase_structure(server, &self.level, pos);
        if let Some(view) = TestInstanceView::read(server, &self.level, pos) {
            let bounding_box = view.get_test_bounding_box();
            if let Some(level) = server.level_mut(&self.level) {
                structure_utils::clear_block_ticks_and_events(level, &bounding_box);
            }
        }
        for listener in self.listeners.borrow().clone() {
            listener.test_structure_loaded(server, self);
        }
    }

    /// `tick(runner)`.
    pub fn tick(self: &Rc<Self>, server: &mut MinecraftServer, runner: &Rc<GameTestRunner>) {
        if self.is_done() {
            return;
        }
        if !self.placed_structure.get() {
            self.fail(translatable!("test.error.ticking_without_structure"));
        }
        if !self.has_block_entity.get() {
            self.fail(translatable!("test.error.missing_block_entity"));
        }
        if self.error.borrow().is_some() {
            self.finish();
        } else {
            if !self.chunks_loaded.get() {
                let view = self
                    .test_block_pos
                    .get()
                    .and_then(|pos| TestInstanceView::read(server, &self.level, pos));
                let all_active = match (&view, server.level(&self.level)) {
                    (Some(view), Some(level)) => view
                        .get_structure_bounding_box()
                        .intersecting_chunks()
                        .all(|chunk| level.are_entities_actually_loaded_and_ticking(chunk)),
                    _ => true,
                };
                if !all_active {
                    return;
                }
            }
            self.chunks_loaded.set(true);
        }
        self.tick_internal(server);
        if self.is_done() {
            for listener in self.listeners.borrow().clone() {
                if self.error.borrow().is_some() {
                    listener.test_failed(server, self, runner);
                } else {
                    listener.test_passed(server, self, runner);
                }
            }
        }
    }

    fn tick_internal(self: &Rc<Self>, server: &mut MinecraftServer) {
        self.tick_count.set(self.tick_count.get() + 1);
        if self.tick_count.get() < 0 {
            return;
        }
        if !self.started.get() {
            self.start_test(server);
        }
        // `runAtTickTimeMap`: every due entry runs once and is removed
        let tick = i64::from(self.tick_count.get());
        let (due, pending): (Vec<_>, Vec<_>) =
            std::mem::take(&mut *self.run_at_tick_time_map.borrow_mut())
                .into_iter()
                .partition(|(time, _)| *time <= tick);
        *self.run_at_tick_time_map.borrow_mut() = pending;
        for (_, runnable) in due {
            let result = self.run_guarded(server, |helper| runnable(helper));
            if let Err(e) = result {
                self.fail_with(e);
            }
        }
        if self.tick_count.get() > self.timeout_ticks {
            let sequences: Vec<Rc<GameTestSequence>> = self.sequences.borrow().clone();
            if sequences.is_empty() {
                self.fail_with(GameTestException::Timeout {
                    message: Box::new(translatable!(
                        "test.error.timeout.no_result",
                        self.test.value.max_ticks()
                    )),
                });
            } else {
                for sequence in &sequences {
                    sequence.tick_and_fail_if_not_complete(server, self, self.tick_count.get());
                }
                if self.error.borrow().is_none() {
                    self.fail_with(GameTestException::Timeout {
                        message: Box::new(translatable!(
                            "test.error.timeout.no_sequences_finished",
                            self.test.value.max_ticks()
                        )),
                    });
                }
            }
        } else {
            let sequences: Vec<Rc<GameTestSequence>> = self.sequences.borrow().clone();
            for sequence in &sequences {
                sequence.tick_and_continue(server, self, self.tick_count.get());
            }
        }
    }

    /// Runs `runnable` with a helper, turning a panic into the `UnknownGameTestException` vanilla wraps a thrown
    /// `Exception` in.
    pub(super) fn run_guarded<F>(
        self: &Rc<Self>,
        server: &mut MinecraftServer,
        f: F,
    ) -> Result<(), GameTestException>
    where
        F: FnOnce(&mut GameTestHelper<'_>) -> Result<(), GameTestException>,
    {
        let this = self.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut helper = GameTestHelper::new(server, &this);
            f(&mut helper)
        }));
        result.unwrap_or_else(|payload| {
            Err(GameTestException::Unknown {
                reason: panic_message(payload),
            })
        })
    }

    fn start_test(self: &Rc<Self>, server: &mut MinecraftServer) {
        if self.started.get() {
            return;
        }
        self.started.set(true);
        self.timer_start.set(Some(Instant::now()));
        if let Some(pos) = self.test_block_pos.get() {
            test_instance_block_entity::set_running(server, &self.level, pos);
        }
        let test = self.test.clone();
        let result = self.run_guarded(server, |helper| test.value.run(helper));
        if let Err(e) = result {
            self.fail_with(e);
        }
    }

    /// `setRunAtTickTime(time, assertAtTickTime)`.
    pub fn set_run_at_tick_time(&self, time: i64, assert_at_tick_time: Runnable) {
        self.run_at_tick_time_map
            .borrow_mut()
            .push((time, assert_at_tick_time));
    }

    /// `id()`.
    pub fn id(&self) -> &Identifier {
        &self.test.key
    }

    /// `getTestBlockPos()`.
    pub fn get_test_block_pos(&self) -> Option<BlockPos> {
        self.test_block_pos.get()
    }

    pub fn level(&self) -> &DimensionKey {
        &self.level
    }

    /// The block entity of this test (`getTestInstanceBlockEntity()`): `None` where vanilla throws.
    pub fn get_test_instance_view(&self, server: &MinecraftServer) -> Option<TestInstanceView> {
        let view = TestInstanceView::read(server, &self.level, self.test_block_pos.get()?)?;
        self.has_block_entity.set(true);
        Some(view)
    }

    /// `getTestOrigin()`: `testInstanceBlockEntity.getStartCorner()`.
    pub fn get_test_origin(&self, server: &MinecraftServer) -> BlockPos {
        self.get_test_instance_view(server)
            .map(|v| v.get_start_corner())
            .unwrap_or(BlockPos::ZERO)
    }

    /// `getStructureBounds()`.
    pub fn get_structure_bounds(&self, server: &MinecraftServer) -> AABB {
        match self.get_test_instance_view(server) {
            Some(view) => view.get_structure_bounds(),
            None => AABB::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn has_succeeded(&self) -> bool {
        self.done.get() && self.error.borrow().is_none()
    }

    pub fn has_failed(&self) -> bool {
        self.error.borrow().is_some()
    }

    pub fn has_started(&self) -> bool {
        self.started.get()
    }

    pub fn is_done(&self) -> bool {
        self.done.get()
    }

    /// `getRunTime()`: milliseconds.
    pub fn get_run_time(&self) -> u128 {
        let running = self
            .timer_start
            .get()
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);
        (self.timer_elapsed.get() + running).as_millis()
    }

    fn finish(&self) {
        if !self.done.get() {
            self.done.set(true);
            if let Some(start) = self.timer_start.take() {
                self.timer_elapsed
                    .set(self.timer_elapsed.get() + start.elapsed());
            }
        }
    }

    /// `succeed()`.
    pub fn succeed(&self, server: &mut MinecraftServer) {
        if self.error.borrow().is_some() {
            return;
        }
        self.finish();
        let bounds = self.get_structure_bounds(server).inflate_scalar(1.0);
        if let Some(level) = server.level_mut(&self.level) {
            structure_utils::discard_non_player_entities(level, &bounds);
        }
    }

    /// `fail(message)`.
    pub fn fail(&self, message: Component) {
        self.fail_with(GameTestException::assert(message, self.tick_count.get()));
    }

    /// `fail(error)`.
    pub fn fail_with(&self, error: GameTestException) {
        *self.error.borrow_mut() = Some(error);
    }

    /// `getError()`.
    pub fn get_error(&self) -> Option<GameTestException> {
        self.error.borrow().clone()
    }

    /// `addListener(listener)`.
    pub fn add_listener(&self, listener: Rc<dyn GameTestListener>) {
        self.listeners.borrow_mut().push(listener);
    }

    /// `prepareTestStructure()`.
    pub fn prepare_test_structure(
        self: &Rc<Self>,
        server: &mut MinecraftServer,
    ) -> Option<Rc<GameTestInfo>> {
        let pos = self
            .test_block_pos
            .get()
            .expect("a test block position was set");
        if self.create_test_instance_block(server, pos, self.extra_rotation) {
            self.has_block_entity.set(true);
            self.place_structure(server);
            Some(self.clone())
        } else {
            None
        }
    }

    /// `createTestInstanceBlock(testPos, rotation, level)`.
    fn create_test_instance_block(
        &self,
        server: &mut MinecraftServer,
        test_pos: BlockPos,
        rotation: Rotation,
    ) -> bool {
        let Some(level) = server.level_mut(&self.level) else {
            return false;
        };
        level.set_block_and_update(test_pos, Block::TEST_INSTANCE_BLOCK.default_state());
        if TestInstanceView::read(server, &self.level, test_pos).is_none() {
            return false;
        }
        let key = self.test.key.clone();
        let size = test_instance_block_entity::get_structure_size(server, &self.level, &key)
            .unwrap_or(Vec3i::new(1, 1, 1));
        test_instance_block_entity::set(
            server,
            &self.level,
            test_pos,
            Data {
                test: Some(key),
                size,
                rotation,
                ignore_entities: false,
                status: Status::Cleared,
                error_message: None,
            },
        );
        true
    }

    /// `getTick()`.
    pub fn get_tick(&self) -> i32 {
        self.tick_count.get()
    }

    /// `createSequence()`.
    pub fn create_sequence(&self) -> Rc<GameTestSequence> {
        let sequence = Rc::new(GameTestSequence::new(self.tick_count.get()));
        self.sequences.borrow_mut().push(sequence.clone());
        sequence
    }

    pub fn is_required(&self) -> bool {
        self.test.value.required()
    }

    pub fn is_optional(&self) -> bool {
        !self.test.value.required()
    }

    pub fn get_structure(&self) -> &Identifier {
        self.test.value.structure()
    }

    /// `getRotation()`.
    pub fn get_rotation(&self) -> Rotation {
        self.test
            .value
            .info()
            .rotation
            .get_rotated(self.extra_rotation)
    }

    pub fn get_test(&self) -> &GameTestInstance {
        &self.test.value
    }

    pub fn get_test_holder(&self) -> &TestHolder {
        &self.test
    }

    pub fn get_timeout_ticks(&self) -> i32 {
        self.timeout_ticks
    }

    pub fn is_flaky(&self) -> bool {
        self.test.value.max_attempts() > 1
    }

    pub fn max_attempts(&self) -> i32 {
        self.test.value.max_attempts()
    }

    pub fn required_successes(&self) -> i32 {
        self.test.value.required_successes()
    }

    pub fn retry_options(&self) -> RetryOptions {
        self.retry_options
    }

    /// `getListeners()`.
    pub fn get_listeners(&self) -> Vec<Rc<dyn GameTestListener>> {
        self.listeners.borrow().clone()
    }

    /// `copyReset()`.
    pub fn copy_reset(&self) -> Rc<GameTestInfo> {
        let copy = GameTestInfo::new(
            self.test.clone(),
            self.extra_rotation,
            self.level.clone(),
            self.retry_options,
        );
        if let Some(pos) = self.test_block_pos.get() {
            copy.set_test_block_pos(Some(pos));
        }
        copy
    }
}

impl std::fmt::Display for GameTestInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(payload) => payload
            .downcast::<&'static str>()
            .map(|s| (*s).to_owned())
            .unwrap_or_else(|_| "panic".to_owned()),
    }
}
