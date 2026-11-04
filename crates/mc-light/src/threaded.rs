//! `ThreadedLevelLightEngine`: the ordered light task queue in front of a [`LevelLightEngine`].
//!
//! Vanilla runs these tasks on a dedicated light thread through its chunk task dispatcher. Here the queue is a plain
//! FIFO owned by whoever runs the engine (a light thread or the game thread): `add_*` mirror the vanilla `addTask`
//! callers, [`ThreadedLevelLightEngine::run_update`] is `runUpdate` (PRE_UPDATE tasks, `runLightUpdates`, POST_UPDATE
//! tasks) and its return value stands for the `CompletableFuture`s vanilla completes in POST_UPDATE.

use crate::chunk_source::LightChunkGetter;
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

pub struct ThreadedLevelLightEngine {
    engine: LevelLightEngine,
    tasks: Vec<Task>,
    next_barrier: u64,
}

impl ThreadedLevelLightEngine {
    /// `new ThreadedLevelLightEngine(chunkGetter, chunkMap, hasSkyLight, ..)`: block light always, sky light on demand.
    pub fn new(level_height: LevelHeight, has_sky_light: bool) -> Self {
        ThreadedLevelLightEngine { engine: LevelLightEngine::new(level_height, true, has_sky_light), tasks: Vec::new(), next_barrier: 0 }
    }

    /// The underlying engine, for queries (`get_*`, `reader`, packet data) and unqueued calls.
    pub fn engine(&self) -> &LevelLightEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut LevelLightEngine {
        &mut self.engine
    }

    pub fn pending_task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Vanilla runs `runUpdate()` inline as soon as the queue holds a full batch.
    pub fn batch_full(&self) -> bool {
        self.tasks.len() >= DEFAULT_BATCH_SIZE
    }

    /// `tryScheduleUpdate` condition: `!lightTasks.isEmpty() || hasLightWork()`.
    pub fn has_pending_work(&self) -> bool {
        !self.tasks.is_empty() || self.engine.has_light_work()
    }

    pub fn check_block(&mut self, pos: BlockPos) {
        self.tasks.push(Task::CheckBlock(pos));
    }

    /// `updateChunkStatus(pos)`.
    pub fn update_chunk_status(&mut self, pos: ChunkPos) {
        self.tasks.push(Task::UpdateChunkStatus(pos));
    }

    pub fn update_section_status(&mut self, pos: SectionPos, section_empty: bool) {
        self.tasks.push(Task::UpdateSectionStatus(pos, section_empty));
    }

    pub fn propagate_light_sources(&mut self, pos: ChunkPos) {
        self.tasks.push(Task::PropagateLightSources(pos));
    }

    pub fn set_light_enabled(&mut self, pos: ChunkPos, enable: bool) {
        self.tasks.push(Task::SetLightEnabled(pos, enable));
    }

    pub fn queue_section_data(&mut self, layer: LightLayer, pos: SectionPos, data: Option<DataLayer>) {
        self.tasks.push(Task::QueueSectionData(layer, pos, data));
    }

    pub fn retain_data(&mut self, pos: ChunkPos, retain: bool) {
        self.tasks.push(Task::RetainData(pos, retain));
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
        self.tasks.push(Task::InitializeLight(chunk.pos, non_empty));
        self.tasks.push(Task::FinishInitializeLight(chunk.pos, lighted));
    }

    /// `lightChunk(chunk, lighted)`: the caller marks the chunk `setLightCorrect(false)` first and sets it to true
    /// when [`LightCompletion::Lit`] arrives.
    pub fn light_chunk(&mut self, pos: ChunkPos, lighted: bool) {
        self.tasks.push(Task::LightChunk(pos, lighted));
        self.tasks.push(Task::FinishLightChunk(pos));
    }

    /// `waitForPendingTasks`: the returned id comes back as [`LightCompletion::Barrier`] once every earlier task ran.
    pub fn wait_for_pending_tasks(&mut self) -> u64 {
        let id = self.next_barrier;
        self.next_barrier += 1;
        self.tasks.push(Task::Barrier(id));
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

    /// `runUpdate()`: up to a batch of tasks — PRE_UPDATE ones run, then the engine updates, then the POST_UPDATE
    /// ones of the batch run and the whole batch is dropped.
    pub fn run_update(&mut self, chunks: &dyn LightChunkGetter) -> Vec<LightCompletion> {
        let total = self.tasks.len().min(DEFAULT_BATCH_SIZE);
        for task in self.tasks.iter_mut().take(total) {
            if !task.is_post_update() {
                Self::run_pre(&mut self.engine, chunks, task);
            }
        }
        self.engine.run_light_updates(chunks);
        let mut completions = Vec::new();
        for task in self.tasks.drain(..total) {
            match task {
                Task::FinishInitializeLight(pos, lighted) => {
                    self.engine.set_light_enabled(chunks, pos, lighted);
                    self.engine.retain_data(pos, false);
                    completions.push(LightCompletion::Initialized(pos));
                }
                Task::FinishLightChunk(pos) => completions.push(LightCompletion::Lit(pos)),
                Task::Barrier(id) => completions.push(LightCompletion::Barrier(id)),
                _ => {}
            }
        }
        completions
    }
}
