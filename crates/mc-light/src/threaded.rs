//! `ThreadedLevelLightEngine`: the ordered light task queue in front of a [`LevelLightEngine`].
//!
//! Vanilla submits every task to `lightTaskDispatcher` (a `ChunkTaskDispatcher`): the task is queued per chunk at the
//! chunk's queue level (`ChunkMap.getChunkQueueLevel`, or `0` for `updateChunkStatus`/`updateSectionStatus`/
//! `queueSectionData`/`retainData`), and the light thread pops chunk after chunk, lowest level first, appending to
//! `lightTasks`. Here the engine is run by whoever owns it (the game thread) and "the light thread got to run" is
//! [`ThreadedLevelLightEngine::run_update`]: it drains every submission through the priority queue into `lightTasks`
//! (running `runUpdate` inline whenever the list reaches a full batch, like `addTask`), then runs `runUpdate` once
//! (PRE_UPDATE tasks, `runLightUpdates`, POST_UPDATE tasks). The return value stands for the `CompletableFuture`s
//! vanilla completes in POST_UPDATE. Vanilla's interleaving of the light thread with the main thread is a race; this
//! is the schedule where the light thread is idle until the main thread polls it.

use crate::chunk_source::LightChunkGetter;
use crate::chunk_task_queue::ChunkTaskPriorityQueue;
use crate::level_light_engine::LevelLightEngine;
use crate::storage::LightLayer;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_world::chunk::{ChunkAccess, LevelHeight};
use mc_world::data_layer::DataLayer;

/// `DEFAULT_BATCH_SIZE` / `taskPerBatch`.
pub const DEFAULT_BATCH_SIZE: usize = 1000;

enum Task {
    CheckBlock(BlockPos),
    UpdateChunkStatus(ChunkPos),
    UpdateSectionStatus(SectionPos, bool),
    PropagateLightSources(ChunkPos),
    SetLightEnabled(ChunkPos, bool),
    QueueSectionData(LightLayer, SectionPos, Option<DataLayer>),
    RetainData(ChunkPos, bool),
    /// `initializeLight` PRE part: sections that are not empty.
    InitializeLight(ChunkPos, Vec<i32>),
    /// `lightChunk` PRE part.
    LightChunk(ChunkPos, bool),
    /// POST_UPDATE: `initializeLight`'s `setLightEnabled(pos, lighted); retainData(pos, false)`.
    FinishInitializeLight(ChunkPos, bool),
    /// POST_UPDATE: `lightChunk`'s completion (the chunk becomes `lightCorrect`).
    FinishLightChunk(ChunkPos),
    /// POST_UPDATE: `waitForPendingTasks`.
    Barrier(u64),
}

impl Task {
    fn is_post_update(&self) -> bool {
        matches!(self, Task::FinishInitializeLight(..) | Task::FinishLightChunk(_) | Task::Barrier(_))
    }
}

/// The level supplier a task is submitted with.
#[derive(Clone, Copy)]
enum SubmitLevel {
    /// `() -> 0`.
    Zero,
    /// `chunkMap.getChunkQueueLevel(pos)`, evaluated when the dispatcher handles the submission.
    Chunk,
}

/// What a POST_UPDATE task completes (the vanilla futures).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightCompletion {
    /// `initializeLight(chunk, lighted)` finished.
    Initialized(ChunkPos),
    /// `lightChunk(chunk, lighted)` finished: set `chunk.setLightCorrect(true)`.
    Lit(ChunkPos),
    /// `waitForPendingTasks(x, z)` with the id returned by [`ThreadedLevelLightEngine::wait_for_pending_tasks`].
    Barrier(u64),
}

/// Result of [`ThreadedLevelLightEngine::run_update`].
#[derive(Debug, Default)]
pub struct UpdateResult {
    /// What `runLightUpdates` returned (queue entries processed).
    pub light_updates_run: i32,
    pub completions: Vec<LightCompletion>,
}

pub struct ThreadedLevelLightEngine {
    engine: LevelLightEngine,
    /// `lightTasks`.
    tasks: Vec<Task>,
    /// Submissions the dispatcher has not handled yet (its `submit` messages), in submission order.
    submitted: Vec<(i64, SubmitLevel, Task)>,
    /// `ChunkTaskDispatcher.queue`; empty between runs, so `onLevelChange` (`resortChunkTasks`) never has tasks to move:
    /// a chunk's level change only shows through the level supplier evaluated when a submission is handled.
    queue: ChunkTaskPriorityQueue<Task>,
    next_barrier: u64,
}

impl ThreadedLevelLightEngine {
    /// `new ThreadedLevelLightEngine(chunkGetter, chunkMap, hasSkyLight, ..)`: block light always, sky light on demand.
    /// `priority_level_count` is `ChunkTaskPriorityQueue.PRIORITY_LEVEL_COUNT` (`ChunkLevel.MAX_LEVEL + 2`).
    pub fn new(level_height: LevelHeight, has_sky_light: bool, priority_level_count: i32) -> Self {
        ThreadedLevelLightEngine {
            engine: LevelLightEngine::new(level_height, true, has_sky_light),
            tasks: Vec::new(),
            submitted: Vec::new(),
            queue: ChunkTaskPriorityQueue::new(priority_level_count),
            next_barrier: 0,
        }
    }

    /// The underlying engine, for queries (`get_*`, `reader`, packet data) and unqueued calls.
    pub fn engine(&self) -> &LevelLightEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut LevelLightEngine {
        &mut self.engine
    }

    /// Tasks in `lightTasks` (already dispatched, not yet run).
    pub fn pending_task_count(&self) -> usize {
        self.tasks.len()
    }

    /// `tryScheduleUpdate` condition (`!lightTasks.isEmpty() || hasLightWork()`) or anything still in the dispatcher:
    /// whether [`Self::run_update`] has something to do.
    pub fn has_pending_work(&self) -> bool {
        !self.tasks.is_empty() || self.dispatcher_has_work() || self.engine.has_light_work()
    }

    /// `lightTaskDispatcher.hasWork()`: unhandled submissions or tasks waiting in the priority queue.
    pub fn dispatcher_has_work(&self) -> bool {
        !self.submitted.is_empty() || self.queue.has_work()
    }

    fn submit(&mut self, pos: ChunkPos, level: SubmitLevel, task: Task) {
        self.submitted.push((pos.pack(), level, task));
    }

    pub fn check_block(&mut self, pos: BlockPos) {
        self.submit(ChunkPos::new(pos.x() >> 4, pos.z() >> 4), SubmitLevel::Chunk, Task::CheckBlock(pos));
    }

    /// `updateChunkStatus(pos)`.
    pub fn update_chunk_status(&mut self, pos: ChunkPos) {
        self.submit(pos, SubmitLevel::Zero, Task::UpdateChunkStatus(pos));
    }

    pub fn update_section_status(&mut self, pos: SectionPos, section_empty: bool) {
        self.submit(ChunkPos::new(pos.x(), pos.z()), SubmitLevel::Zero, Task::UpdateSectionStatus(pos, section_empty));
    }

    pub fn propagate_light_sources(&mut self, pos: ChunkPos) {
        self.submit(pos, SubmitLevel::Chunk, Task::PropagateLightSources(pos));
    }

    pub fn set_light_enabled(&mut self, pos: ChunkPos, enable: bool) {
        self.submit(pos, SubmitLevel::Chunk, Task::SetLightEnabled(pos, enable));
    }

    pub fn queue_section_data(&mut self, layer: LightLayer, pos: SectionPos, data: Option<DataLayer>) {
        self.submit(ChunkPos::new(pos.x(), pos.z()), SubmitLevel::Zero, Task::QueueSectionData(layer, pos, data));
    }

    pub fn retain_data(&mut self, pos: ChunkPos, retain: bool) {
        self.submit(pos, SubmitLevel::Zero, Task::RetainData(pos, retain));
    }

    /// The load side of `SerializableChunkData.read` (`retainData` + `queueSectionData` per layer), as queued tasks.
    pub fn load_chunk_light(&mut self, pos: ChunkPos, light: Vec<(i32, Option<DataLayer>, Option<DataLayer>)>) {
        let mut loaded_any_light = false;
        for (section_y, block_light, sky_light) in light {
            if block_light.is_none() && sky_light.is_none() {
                continue;
            }
            if !loaded_any_light {
                self.retain_data(pos, true);
                loaded_any_light = true;
            }
            let section_pos = SectionPos::of_chunk(&pos, section_y);
            if let Some(block_light) = block_light {
                self.queue_section_data(LightLayer::Block, section_pos, Some(block_light));
            }
            if let Some(sky_light) = sky_light {
                self.queue_section_data(LightLayer::Sky, section_pos, Some(sky_light));
            }
        }
    }

    /// `initializeLight(chunk, lighted)`: reports the non-empty sections now; the returned completion
    /// ([`LightCompletion::Initialized`]) arrives from `run_update` after `setLightEnabled(pos, lighted)`.
    pub fn initialize_light(&mut self, chunk: &ChunkAccess, lighted: bool) {
        let non_empty = chunk
            .sections
            .iter()
            .enumerate()
            .filter(|(_, section)| !section.has_only_air())
            .map(|(index, _)| chunk.level_height.section_y_from_section_index(index as i32))
            .collect();
        self.submit(chunk.pos, SubmitLevel::Chunk, Task::InitializeLight(chunk.pos, non_empty));
        self.submit(chunk.pos, SubmitLevel::Chunk, Task::FinishInitializeLight(chunk.pos, lighted));
    }

    /// `lightChunk(chunk, lighted)`: the caller marks the chunk `setLightCorrect(false)` first and sets it to true
    /// when [`LightCompletion::Lit`] arrives.
    pub fn light_chunk(&mut self, pos: ChunkPos, lighted: bool) {
        self.submit(pos, SubmitLevel::Chunk, Task::LightChunk(pos, lighted));
        self.submit(pos, SubmitLevel::Chunk, Task::FinishLightChunk(pos));
    }

    /// `waitForPendingTasks`: the returned id comes back as [`LightCompletion::Barrier`] once every earlier task ran.
    pub fn wait_for_pending_tasks(&mut self, pos: ChunkPos) -> u64 {
        let id = self.next_barrier;
        self.next_barrier += 1;
        self.submit(pos, SubmitLevel::Chunk, Task::Barrier(id));
        id
    }

    fn run_pre(engine: &mut LevelLightEngine, chunks: &dyn LightChunkGetter, task: &mut Task) {
        let level = engine.level_height();
        match task {
            Task::CheckBlock(pos) => engine.check_block(*pos),
            Task::UpdateChunkStatus(pos) => {
                engine.retain_data(*pos, false);
                engine.set_light_enabled(chunks, *pos, false);
                for section_y in engine.get_min_light_section()..engine.get_max_light_section() {
                    engine.queue_section_data(LightLayer::Block, SectionPos::of_chunk(pos, section_y), None);
                    engine.queue_section_data(LightLayer::Sky, SectionPos::of_chunk(pos, section_y), None);
                }
                for section_y in level.min_section_y()..=level.max_section_y() {
                    engine.update_section_status(SectionPos::of_chunk(pos, section_y), true);
                }
            }
            Task::UpdateSectionStatus(pos, empty) => engine.update_section_status(*pos, *empty),
            Task::PropagateLightSources(pos) => engine.propagate_light_sources(chunks, *pos),
            Task::SetLightEnabled(pos, enable) => engine.set_light_enabled(chunks, *pos, *enable),
            Task::QueueSectionData(layer, pos, data) => engine.queue_section_data(*layer, *pos, data.take()),
            Task::RetainData(pos, retain) => engine.retain_data(*pos, *retain),
            Task::InitializeLight(pos, section_ys) => {
                for &section_y in section_ys.iter() {
                    engine.update_section_status(SectionPos::of_chunk(pos, section_y), false);
                }
            }
            Task::LightChunk(pos, lighted) => {
                if !*lighted {
                    engine.propagate_light_sources(chunks, *pos);
                }
            }
            Task::FinishInitializeLight(..) | Task::FinishLightChunk(_) | Task::Barrier(_) => {}
        }
    }

    /// The light thread runs: every submission goes through the dispatcher into `lightTasks`, then `runUpdate`.
    /// `queue_level` is `ChunkMap.getChunkQueueLevel(pos)`.
    pub fn run_update(&mut self, chunks: &dyn LightChunkGetter, queue_level: &dyn Fn(i64) -> i32) -> UpdateResult {
        let mut result = UpdateResult::default();
        for (pos, level, task) in std::mem::take(&mut self.submitted) {
            let level = match level {
                SubmitLevel::Zero => 0,
                SubmitLevel::Chunk => queue_level(pos),
            };
            self.queue.submit(task, pos, level);
        }
        while let Some((_, tasks)) = self.queue.pop() {
            for task in tasks {
                self.tasks.push(task);
                if self.tasks.len() >= DEFAULT_BATCH_SIZE {
                    self.run_batch(chunks, &mut result);
                }
            }
        }
        self.run_batch(chunks, &mut result);
        result
    }

    /// `runUpdate()`: up to a batch of tasks — PRE_UPDATE ones run, then the engine updates, then the POST_UPDATE
    /// ones of the batch run and the whole batch is dropped.
    fn run_batch(&mut self, chunks: &dyn LightChunkGetter, result: &mut UpdateResult) {
        let total = self.tasks.len().min(DEFAULT_BATCH_SIZE);
        for task in self.tasks.iter_mut().take(total) {
            if !task.is_post_update() {
                Self::run_pre(&mut self.engine, chunks, task);
            }
        }
        result.light_updates_run += self.engine.run_light_updates(chunks);
        for task in self.tasks.drain(..total) {
            match task {
                Task::FinishInitializeLight(pos, lighted) => {
                    self.engine.set_light_enabled(chunks, pos, lighted);
                    self.engine.retain_data(pos, false);
                    result.completions.push(LightCompletion::Initialized(pos));
                }
                Task::FinishLightChunk(pos) => result.completions.push(LightCompletion::Lit(pos)),
                Task::Barrier(id) => result.completions.push(LightCompletion::Barrier(id)),
                _ => {}
            }
        }
    }
}

/// Same hook, but queued like `ThreadedLevelLightEngine.checkBlock`/`updateSectionStatus`.
impl mc_world::chunk::LightEngineHook for ThreadedLevelLightEngine {
    fn update_section_status(&mut self, pos: BlockPos, section_empty: bool) {
        ThreadedLevelLightEngine::update_section_status(self, SectionPos::of_block_pos(&pos), section_empty);
    }

    fn check_block(&mut self, pos: BlockPos) {
        ThreadedLevelLightEngine::check_block(self, pos);
    }
}
