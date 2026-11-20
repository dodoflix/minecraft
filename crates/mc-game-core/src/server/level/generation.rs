//! `ChunkGenerationTask` and the `GeneratingChunkMap` side of `ChunkMap`: scheduling status layers through the
//! generation/loading pyramids, applying steps (on the game thread, the worldgen thread, the IO/parse pool or the
//! light engine) and completing the per-status futures.

use super::chunk_holder::{ChunkHolder, ChunkSlot, FutureState, Waiter};
use super::chunk_io::WorkerMessage;
use super::chunk_map::{ChunkMap, ChunkMapEvent, MainTask};
use super::chunk_status_tasks::{
    BatchResult, GenBatch, GenJob, runs_on_background_executor, runs_on_worldgen_thread,
};
use super::world_gen::{GenChunk, LightEvent, OldChunk};
use crate::world::level::chunk::status::{
    ChunkStatusTask, ChunkStep, GENERATION_PYRAMID, LOADING_PYRAMID,
};
use mc_core::pos::{ChunkPos, SectionPos};
use mc_light::LightCompletion;
use mc_world::chunk::{ChunkAccess, LevelHeight, ProtoChunk, StructureData};
use mc_world::palette::PalettedContainerFactory;
use mc_world::section::LevelChunkSection;
use mc_world::status::ChunkStatus;
use mc_world::upgrade_data::UpgradeData;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ChunkGenerationTask {
    pos: ChunkPos,
    scheduled_status: Option<ChunkStatus>,
    pub target_status: ChunkStatus,
    marked_for_cancellation: bool,
    /// The futures of the layer being waited for, as `(chunk key, status)`.
    scheduled_layer: Vec<(i64, ChunkStatus)>,
    needs_generation: bool,
    /// Radius of the claimed chunk square (`cache`).
    cache_range: i32,
    /// Worldgen-thread steps collected while scheduling the current layer.
    batch_jobs: Vec<GenJob>,
    released: bool,
}

impl ChunkGenerationTask {
    pub fn center_key(&self) -> i64 {
        self.pos.pack()
    }

    pub fn mark_for_cancellation(&mut self) {
        self.marked_for_cancellation = true;
    }
}

impl ChunkMap {
    // ------------------------------------------------------------------ scheduling

    /// `GenerationChunkHolder.scheduleChunkGenerationTask(status, scheduler)`.
    pub(crate) fn schedule_chunk_generation_task(
        &mut self,
        key: i64,
        status: ChunkStatus,
    ) -> FutureState {
        let Some(holder) = self.holders.holder_mut(key) else {
            return FutureState::Unloaded;
        };
        if holder.is_status_disallowed(status) {
            return FutureState::Unloaded;
        }
        let state = holder.get_or_create_future(status);
        if state == FutureState::Done {
            return state;
        }
        let task_id = holder.task;
        let needs_reschedule = match task_id {
            None => true,
            Some(id) => self
                .tasks
                .get(&id)
                .is_none_or(|t| status.is_after(t.target_status)),
        };
        if needs_reschedule {
            self.reschedule_chunk_task(key, Some(status));
        }
        state
    }

    /// `rescheduleChunkTask(scheduler, status)`.
    pub(crate) fn reschedule_chunk_task(&mut self, key: i64, status: Option<ChunkStatus>) {
        let new_task = status.map(|s| self.schedule_generation_task(s, ChunkPos::unpack(key)));
        let old = match self.holders.holder_mut(key) {
            Some(h) => std::mem::replace(&mut h.task, new_task),
            None => None,
        };
        if let Some(old) = old
            && let Some(t) = self.tasks.get_mut(&old)
        {
            t.mark_for_cancellation();
        }
    }

    /// `ChunkMap.scheduleGenerationTask` + `ChunkGenerationTask.create`.
    pub(crate) fn schedule_generation_task(&mut self, target: ChunkStatus, pos: ChunkPos) -> u64 {
        let range = GENERATION_PYRAMID
            .get_step_to(target)
            .get_accumulated_radius_of(ChunkStatus::Empty);
        for x in pos.x - range..=pos.x + range {
            for z in pos.z - range..=pos.z + range {
                // `acquireGeneration`
                self.holders
                    .updating
                    .get_mut(&ChunkPos::pack_xz(x, z))
                    .expect("holder in the generation range")
                    .increase_generation_ref_count();
            }
        }
        let id = self.next_task_id;
        self.next_task_id += 1;
        self.tasks.insert(
            id,
            ChunkGenerationTask {
                pos,
                scheduled_status: None,
                target_status: target,
                marked_for_cancellation: false,
                scheduled_layer: Vec::new(),
                needs_generation: false,
                cache_range: range,
                batch_jobs: Vec::new(),
                released: false,
            },
        );
        self.pending_generation_tasks.push(id);
        id
    }

    /// `runGenerationTasks()`.
    pub fn run_generation_tasks(&mut self) {
        for id in std::mem::take(&mut self.pending_generation_tasks) {
            self.queue_task_slice(id);
            // direct executors (oracle mode) run each submitted task, and what its steps wake, before the next is submitted
            if self.worldgen.is_inline() {
                self.pump_worldgen();
            }
        }
        self.pump_worldgen();
    }

    /// Hand the next queued task slice to the "executor" while the worldgen thread is idle.
    pub(crate) fn pump_worldgen(&mut self) {
        while !self.worldgen_busy {
            let Some((_, tasks)) = self.worldgen_queue.pop() else {
                // the worker has nothing queued: the light update gets its turn (an inline worldgen thread, the oracle's
                // direct executors, runs light once per poll in `poll_main_task` instead)
                if !self.worldgen.is_inline() && self.light.has_pending_work() {
                    self.run_light_updates();
                    self.process_ready();
                    if self.worldgen_queue.has_work() {
                        continue;
                    }
                }
                return;
            };
            for id in tasks {
                self.run_generation_task_slice(id);
            }
            // vanilla's direct executors run the `thenRun` resubmissions of the futures a chunk's tasks completed before the
            // dispatcher polls the next chunk's tasks
            if self.worldgen.is_inline() {
                self.process_ready();
            }
        }
    }

    /// One `runUntilWait()` of a task (a worldgen executor slice).
    pub(crate) fn run_generation_task_slice(&mut self, id: u64) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let waiting_for = self.run_until_wait(id);
        let inline_result = self.flush_batch(id);
        if let Some((key, status)) = waiting_for
            && let Some(h) = self.holders.holder_mut(key)
        {
            h.add_waiter(status, Waiter::Task(id));
        }
        // a batch that ran inline completes the futures the task just registered on
        if let Some(message) = inline_result {
            self.handle_worker_message(message);
        }
    }

    fn run_until_wait(&mut self, id: u64) -> Option<(i64, ChunkStatus)> {
        loop {
            if let Some(w) = self.wait_for_scheduled_layer(id) {
                return Some(w);
            }
            let t = self.tasks.get(&id)?;
            if t.marked_for_cancellation || t.scheduled_status == Some(t.target_status) {
                self.release_claim(id);
                return None;
            }
            self.schedule_next_layer(id);
        }
    }

    fn schedule_next_layer(&mut self, id: u64) {
        let (scheduled, _target, needs_generation) = {
            let t = &self.tasks[&id];
            (t.scheduled_status, t.target_status, t.needs_generation)
        };
        let status_to_schedule = match scheduled {
            None => ChunkStatus::Empty,
            Some(s)
                if !needs_generation
                    && s == ChunkStatus::Empty
                    && !self.can_load_without_generation(id) =>
            {
                self.tasks.get_mut(&id).expect("task").needs_generation = true;
                ChunkStatus::Empty
            }
            Some(s) => ChunkStatus::ALL[s.index() as usize + 1],
        };
        let needs_generation = self.tasks[&id].needs_generation;
        self.schedule_layer(id, status_to_schedule, needs_generation);
        self.tasks.get_mut(&id).expect("task").scheduled_status = Some(status_to_schedule);
    }

    fn release_claim(&mut self, id: u64) {
        let Some(mut task) = self.tasks.remove(&id) else {
            return;
        };
        if task.released {
            return;
        }
        task.released = true;
        let center = task.center_key();
        if let Some(h) = self.holders.holder_mut(center)
            && h.task == Some(id)
        {
            h.task = None;
        }
        let range = task.cache_range;
        for x in task.pos.x - range..=task.pos.x + range {
            for z in task.pos.z - range..=task.pos.z + range {
                let key = ChunkPos::pack_xz(x, z);
                if let Some(h) = self.holders.holder_mut(key) {
                    h.decrease_generation_ref_count();
                }
                self.check_unload_ready(key);
            }
        }
    }

    fn can_load_without_generation(&self, id: u64) -> bool {
        let task = &self.tasks[&id];
        if task.target_status == ChunkStatus::Empty {
            return true;
        }
        let Some(center) = self.holders.holder(task.center_key()) else {
            return false;
        };
        let Some(highest_generated) = center.get_persisted_status() else {
            return false;
        };
        if highest_generated.is_before(task.target_status) {
            return false;
        }
        let deps = &LOADING_PYRAMID
            .get_step_to(task.target_status)
            .accumulated_dependencies;
        let range = deps.get_radius();
        for x in task.pos.x - range..=task.pos.x + range {
            for z in task.pos.z - range..=task.pos.z + range {
                let distance = task.pos.get_chessboard_distance_xz(x, z);
                let required = deps.get(distance);
                let persisted = self
                    .holders
                    .holder(ChunkPos::pack_xz(x, z))
                    .and_then(ChunkHolder::get_persisted_status);
                if persisted.is_none_or(|p| p.is_before(required)) {
                    return false;
                }
            }
        }
        true
    }

    fn schedule_layer(&mut self, id: u64, status: ChunkStatus, needs_generation: bool) {
        let (pos, target) = {
            let t = &self.tasks[&id];
            (t.pos, t.target_status)
        };
        let pyramid = if needs_generation {
            &*GENERATION_PYRAMID
        } else {
            &*LOADING_PYRAMID
        };
        let radius = pyramid
            .get_step_to(target)
            .get_accumulated_radius_of(status);
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                let marked = self.tasks[&id].marked_for_cancellation;
                if marked
                    || !self.schedule_chunk_in_layer(
                        id,
                        status,
                        needs_generation,
                        ChunkPos::pack_xz(x, z),
                    )
                {
                    return;
                }
            }
        }
    }

    fn schedule_chunk_in_layer(
        &mut self,
        id: u64,
        status: ChunkStatus,
        needs_generation: bool,
        key: i64,
    ) -> bool {
        let persisted = self
            .holders
            .holder(key)
            .and_then(ChunkHolder::get_persisted_status);
        let generate = persisted.is_some_and(|p| status.is_after(p));
        let pyramid = if generate {
            &*GENERATION_PYRAMID
        } else {
            &*LOADING_PYRAMID
        };
        assert!(
            !(generate && !needs_generation),
            "Can't load chunk, but didn't expect to need to generate"
        );
        let step = pyramid.get_step_to(status).clone();
        match self.apply_step(id, key, &step) {
            FutureState::Pending | FutureState::Absent => {
                self.tasks
                    .get_mut(&id)
                    .expect("task")
                    .scheduled_layer
                    .push((key, status));
                true
            }
            FutureState::Done => true,
            FutureState::Unloaded => {
                self.tasks
                    .get_mut(&id)
                    .expect("task")
                    .mark_for_cancellation();
                false
            }
        }
    }

    /// `waitForScheduledLayer()`: the future still pending, if any.
    fn wait_for_scheduled_layer(&mut self, id: u64) -> Option<(i64, ChunkStatus)> {
        loop {
            let last = *self.tasks.get(&id)?.scheduled_layer.last()?;
            let (key, status) = last;
            let state = match self.holders.holder(key) {
                None => FutureState::Unloaded,
                Some(h) if h.future_state(status) == FutureState::Done => FutureState::Done,
                Some(h) if h.is_status_disallowed(status) => FutureState::Unloaded,
                Some(h) => h.future_state(status),
            };
            // A layer whose worldgen steps were not sent yet is "waiting" on that batch.
            // `Absent` for a scheduled future means it was failed and cleared (`failAndClearPendingFuture`, which
            // completes it with UNLOADED_CHUNK): the task must see the failure, not wait for a future nobody completes.
            if state == FutureState::Pending {
                return Some(last);
            }
            let state = if state == FutureState::Absent {
                FutureState::Unloaded
            } else {
                state
            };
            let t = self.tasks.get_mut(&id)?;
            t.scheduled_layer.pop();
            if state == FutureState::Unloaded {
                t.mark_for_cancellation();
            }
        }
    }

    // ------------------------------------------------------------------ steps

    /// `GenerationChunkHolder.applyStep(step, chunkMap, cache)`.
    fn apply_step(&mut self, id: u64, key: i64, step: &ChunkStep) -> FutureState {
        let target = step.target_status;
        let Some(holder) = self.holders.holder_mut(key) else {
            return FutureState::Unloaded;
        };
        if holder.is_status_disallowed(target) {
            return FutureState::Unloaded;
        }
        if holder.acquire_status_bump(target) {
            holder.get_or_create_future(target);
            self.start_step(id, key, step);
        }
        match self.holders.holder_mut(key) {
            Some(h) => h.get_or_create_future(target),
            None => FutureState::Unloaded,
        }
    }

    /// `ChunkMap.applyStep` + `ChunkStep.apply`.
    fn start_step(&mut self, id: u64, key: i64, step: &ChunkStep) {
        let target = step.target_status;
        if target == ChunkStatus::Empty {
            self.start_chunk_load(key);
            return;
        }
        let holder = self.holders.holder(key).expect("holder");
        assert!(
            holder.has_chunk_unchecked(target.parent()),
            "Parent chunk missing"
        );
        match step.task {
            ChunkStatusTask::PassThrough => self.complete_step(key, target),
            ChunkStatusTask::LoadStructureStarts => {
                if let Some(ChunkSlot::Proto(p)) = self.holders.holder(key).map(|h| &h.slot) {
                    self.gen_context.hooks.on_structure_starts_available(p);
                }
                self.complete_step(key, target);
            }
            task if runs_on_worldgen_thread(task) => {
                self.tasks
                    .get_mut(&id)
                    .expect("task")
                    .batch_jobs
                    .push(GenJob {
                        pos: ChunkPos::unpack(key),
                        step: step.clone(),
                    });
            }
            ChunkStatusTask::InitializeLight => {
                let holder = self.holders.holder_mut(key).expect("holder");
                let status = holder.get_persisted_status().unwrap_or(ChunkStatus::Empty);
                match &mut holder.slot {
                    ChunkSlot::Proto(p) => {
                        p.access.initialize_light_sources();
                        p.set_light_engine_attached(true);
                        let lighted =
                            status.is_or_after(ChunkStatus::Light) && p.access.is_light_correct();
                        self.light.initialize_light(&p.access, lighted);
                    }
                    ChunkSlot::Level(l) => {
                        l.data.access.initialize_light_sources();
                        let lighted = l.data.access.is_light_correct();
                        self.light.initialize_light(&l.data.access, lighted);
                    }
                    _ => panic!("light step on a lent chunk"),
                }
                self.dispatch_light();
            }
            ChunkStatusTask::Light => {
                let holder = self.holders.holder_mut(key).expect("holder");
                let status = holder.get_persisted_status().unwrap_or(ChunkStatus::Empty);
                let (lighted, pos) = match &mut holder.slot {
                    ChunkSlot::Proto(p) => {
                        let lighted =
                            status.is_or_after(ChunkStatus::Light) && p.access.is_light_correct();
                        p.access.set_light_correct(false);
                        (lighted, p.pos())
                    }
                    ChunkSlot::Level(l) => {
                        let lighted = l.data.access.is_light_correct();
                        l.data.access.set_light_correct(false);
                        (lighted, l.pos())
                    }
                    _ => panic!("light step on a lent chunk"),
                };
                self.light.light_chunk(pos, lighted);
                self.dispatch_light();
            }
            ChunkStatusTask::Full => self.main_tasks.push_back(MainTask::FullStep(key)),
            other => unreachable!("{other:?}"),
        }
    }

    /// `completeFuture(status, chunk)` (+ `completeChunkGeneration`'s status bump).
    pub(crate) fn complete_step(&mut self, key: i64, status: ChunkStatus) {
        let Some(holder) = self.holders.holder_mut(key) else {
            return;
        };
        if let ChunkSlot::Proto(p) = &mut holder.slot
            && p.get_persisted_status().is_before(status)
        {
            p.set_persisted_status(status);
        }
        let cached = holder.get_persisted_status();
        let slot_status = match &holder.slot {
            ChunkSlot::Proto(p) => Some(p.get_persisted_status()),
            ChunkSlot::Level(_) => Some(ChunkStatus::Full),
            _ => cached,
        };
        holder.set_persisted_status_cache(slot_status);
        let woken = holder.complete_future(status);
        self.wake(key, woken);
    }

    pub(crate) fn on_light_event(&mut self, event: LightCompletion) {
        match event {
            LightCompletion::Initialized(pos) => {
                self.complete_step(pos.pack(), ChunkStatus::InitializeLight)
            }
            LightCompletion::Lit(pos) => {
                if let Some(h) = self.holders.holder_mut(pos.pack()) {
                    match &mut h.slot {
                        ChunkSlot::Proto(p) => p.access.set_light_correct(true),
                        ChunkSlot::Level(l) => l.data.access.set_light_correct(true),
                        _ => {}
                    }
                }
                self.complete_step(pos.pack(), ChunkStatus::Light);
            }
            LightCompletion::Barrier(id) => {
                self.light_barrier_pending.remove(&id);
            }
        }
    }

    // ------------------------------------------------------------------ worldgen batches

    /// Send the collected worldgen steps of a task's layer to the executors: the steps vanilla runs on
    /// `Util.backgroundExecutor()` (biomes, terrain) as one background batch per chunk, the others to the worldgen thread;
    /// the result of an inline batch comes back.
    fn flush_batch(&mut self, id: u64) -> Option<WorkerMessage> {
        let task = self.tasks.get_mut(&id)?;
        if task.batch_jobs.is_empty() {
            return None;
        }
        let jobs = std::mem::take(&mut task.batch_jobs);
        let (background, jobs): (Vec<GenJob>, Vec<GenJob>) = if self.worldgen.runs_in_parallel() {
            jobs.into_iter()
                .partition(|job| runs_on_background_executor(job.step.task))
        } else {
            (Vec::new(), jobs)
        };
        for job in background {
            self.submit_background_job(job);
        }
        if jobs.is_empty() {
            return None;
        }
        let mut chunks: HashMap<i64, GenChunk> = HashMap::new();
        let mut structure_views: HashMap<i64, Arc<StructureData>> = HashMap::new();
        for job in &jobs {
            let read_radius = job.step.direct_dependencies.get_radius();
            let lend_radius = read_radius
                .min(1)
                .max(job.step.block_state_write_radius.max(0));
            for x in job.pos.x - read_radius..=job.pos.x + read_radius {
                for z in job.pos.z - read_radius..=job.pos.z + read_radius {
                    let key = ChunkPos::pack_xz(x, z);
                    let distance = job.pos.get_chessboard_distance_xz(x, z);
                    if distance <= lend_radius {
                        if chunks.contains_key(&key) {
                            continue;
                        }
                        match self.lend_chunk(key) {
                            Some(chunk) => {
                                chunks.insert(key, chunk);
                            }
                            // lent to a background job (its structure starts are final): readable, not writable
                            None => {
                                if let Some(view) = self.structure_view(key) {
                                    structure_views.entry(key).or_insert(view);
                                }
                            }
                        }
                    } else if !structure_views.contains_key(&key)
                        && let Some(view) = self.structure_view(key)
                    {
                        structure_views.insert(key, view);
                    }
                }
            }
        }
        let mut old_chunks = HashMap::new();
        for job in &jobs {
            if is_blending_step(job.step.task) {
                self.snapshot_old_chunks(job.pos, &chunks, &mut old_chunks);
            }
        }
        let batch_id = self.next_batch_id;
        self.next_batch_id += 1;
        self.in_flight_batch = Some(batch_id);
        self.worldgen_busy = true;
        // an inline batch comes back as the return value, not as a worker message
        let seq = if self.worldgen.is_inline() {
            0
        } else {
            self.next_result_seq()
        };
        self.worldgen.submit(
            self.gen_context.clone(),
            GenBatch {
                id: batch_id,
                jobs,
                chunks,
                structure_views,
                old_chunks,
            },
            seq,
        )
    }

    /// `ChunkStatusTasks.generateBiomes` / `buildTerrain`: the chunk goes to the background pool alone. Its neighbours are
    /// only read: their biomes (`collectPossibleBiomes`, the biome manager) and every structure start within reach, both of
    /// which are final when the step may start, however busy another job keeps the neighbour.
    fn submit_background_job(&mut self, job: GenJob) {
        let key = job.pos.pack();
        let center = self
            .lend_chunk(key)
            .expect("the chunk of a background step is a proto chunk");
        let mut chunks = HashMap::from([(key, center)]);
        let mut structure_views = HashMap::new();
        if job.step.task == ChunkStatusTask::BuildTerrain {
            let radius = job.step.direct_dependencies.get_radius();
            for x in job.pos.x - radius..=job.pos.x + radius {
                for z in job.pos.z - radius..=job.pos.z + radius {
                    let neighbour = ChunkPos::pack_xz(x, z);
                    if neighbour == key {
                        continue;
                    }
                    if let Some(view) = self.structure_view(neighbour) {
                        structure_views.insert(neighbour, view);
                    }
                    if job.pos.get_chessboard_distance_xz(x, z) <= TERRAIN_BIOME_RADIUS
                        && let Some(biomes) = self.biome_view(neighbour)
                    {
                        chunks.insert(neighbour, GenChunk::Shared(biomes));
                    }
                }
            }
        }
        let mut old_chunks = HashMap::new();
        if is_blending_step(job.step.task) {
            self.snapshot_old_chunks(job.pos, &chunks, &mut old_chunks);
        }
        let batch_id = self.next_batch_id;
        self.next_batch_id += 1;
        self.background_batches.insert(batch_id);
        let seq = self.next_result_seq();
        self.worldgen.submit_background(
            self.gen_context.clone(),
            GenBatch {
                id: batch_id,
                jobs: vec![job],
                chunks,
                structure_views,
                old_chunks,
            },
            seq,
        );
    }

    /// Move the chunk out of its holder for a worker (a full chunk is cloned as a read-only snapshot instead).
    fn lend_chunk(&mut self, key: i64) -> Option<GenChunk> {
        let holder = self.holders.holder_mut(key)?;
        match std::mem::take(&mut holder.slot) {
            ChunkSlot::Proto(p) => {
                holder.lent_structures = Some(Arc::new(p.access.structures.clone()));
                holder.lent_old = old_view(&p, self.level_height, &self.factory).map(Arc::new);
                if holder.biome_view.is_none()
                    && p.get_persisted_status().is_or_after(ChunkStatus::Biomes)
                {
                    holder.biome_view = Some(Arc::new(biome_only_chunk(
                        &p.access,
                        self.level_height,
                        &self.factory,
                    )));
                }
                holder.slot = ChunkSlot::Loaned;
                Some(GenChunk::Loaned(p))
            }
            ChunkSlot::Level(l) => {
                let snapshot = level_chunk_snapshot(&l, self.level_height, &self.factory);
                holder.slot = ChunkSlot::Level(l);
                Some(GenChunk::Snapshot(Box::new(snapshot)))
            }
            other => {
                holder.slot = other;
                None
            }
        }
    }

    /// The chunks with blending data within `Blender`'s reach of `center` that `lent` does not hold in full, as the data
    /// `Blender.of(region)` reads from them (a chunk that is lent to another job is read as it was when it was lent).
    fn snapshot_old_chunks(
        &self,
        center: ChunkPos,
        lent: &HashMap<i64, GenChunk>,
        out: &mut HashMap<i64, OldChunk>,
    ) {
        for x in center.x - BLENDING_REACH..=center.x + BLENDING_REACH {
            for z in center.z - BLENDING_REACH..=center.z + BLENDING_REACH {
                let key = ChunkPos::pack_xz(x, z);
                // a biome-only view of the neighbour has no blending data: it is replaced here
                if matches!(
                    lent.get(&key),
                    Some(GenChunk::Loaned(_) | GenChunk::Snapshot(_))
                ) || out.contains_key(&key)
                {
                    continue;
                }
                let Some(holder) = self.holders.holder(key) else {
                    continue;
                };
                let chunk = match &holder.slot {
                    ChunkSlot::Level(l) => {
                        let access = &l.data.access;
                        if access.blending_data.is_none() {
                            continue;
                        }
                        old_snapshot(
                            access,
                            ChunkStatus::Full,
                            None,
                            || level_chunk_snapshot(l, self.level_height, &self.factory),
                            self.level_height,
                            &self.factory,
                        )
                    }
                    ChunkSlot::Proto(p) => match old_view(p, self.level_height, &self.factory) {
                        Some(view) => view,
                        None => continue,
                    },
                    ChunkSlot::Loaned => match &holder.lent_old {
                        Some(view) => (**view).clone(),
                        None => continue,
                    },
                    _ => continue,
                };
                let calculated_before = chunk
                    .access
                    .blending_data
                    .as_ref()
                    .is_some_and(|data| data.has_calculated_data);
                out.insert(
                    key,
                    OldChunk {
                        chunk: Box::new(chunk),
                        calculated_before,
                    },
                );
            }
        }
    }

    /// The structure starts and references of the chunk, also while it is lent.
    fn structure_view(&self, key: i64) -> Option<Arc<StructureData>> {
        let holder = self.holders.holder(key)?;
        match &holder.slot {
            ChunkSlot::Proto(p) => Some(Arc::new(p.access.structures.clone())),
            ChunkSlot::Level(l) => Some(Arc::new(l.data.access.structures.clone())),
            _ => holder.lent_structures.clone(),
        }
    }

    /// The biomes of the chunk (a chunk that reached `BIOMES`), also while it is lent.
    fn biome_view(&mut self, key: i64) -> Option<Arc<ProtoChunk>> {
        let holder = self.holders.holder_mut(key)?;
        match &holder.slot {
            ChunkSlot::Proto(p) => {
                if holder.biome_view.is_none()
                    && p.get_persisted_status().is_or_after(ChunkStatus::Biomes)
                {
                    holder.biome_view = Some(Arc::new(biome_only_chunk(
                        &p.access,
                        self.level_height,
                        &self.factory,
                    )));
                }
                holder.biome_view.clone()
            }
            ChunkSlot::Level(l) => Some(Arc::new(biome_only_chunk(
                &l.data.access,
                self.level_height,
                &self.factory,
            ))),
            _ => holder.biome_view.clone(),
        }
    }

    /// A worldgen batch finished: take the chunks back and complete the futures of its steps.
    pub(crate) fn on_batch_done(&mut self, result: BatchResult) {
        // a background batch overlaps the serial thread's batch: only the serial one clears its flag
        if !self.background_batches.remove(&result.id) {
            self.worldgen_busy = false;
            self.in_flight_batch = None;
        }
        // the `getServer().execute(...)` tasks of `updatePOIOnBlockStateChange`, in call order
        for (pos, old_type, new_type) in result.poi_changes {
            if old_type.is_some() {
                self.poi_manager.remove(pos);
                self.events.push(ChunkMapEvent::PoiRemoved(pos));
            }
            if let Some(poi_type) = new_type
                && let Some(record) = self.poi_manager.add(pos, poi_type)
            {
                self.events.push(ChunkMapEvent::PoiAdded(record));
            }
        }
        // the `lightEngine.checkBlock/updateSectionStatus` calls of `ProtoChunk.setBlockState`, in call order
        for event in result.light_events {
            match event {
                LightEvent::SectionStatus(pos, empty) => self
                    .light
                    .update_section_status(SectionPos::of_block_pos(&pos), empty),
                LightEvent::CheckBlock(pos) => self.light.check_block(pos),
            }
        }
        for (key, mut chunk) in result.chunks {
            if let Some(holder) = self.holders.holder_mut(key) {
                holder.set_persisted_status_cache(Some(chunk.get_persisted_status()));
                holder.lent_structures = None;
                holder.lent_old = None;
                // start rewrites that arrived while the chunk was lent
                for (structure, tag) in std::mem::take(&mut holder.pending_start_updates) {
                    if let Some(start) = chunk.access.structures.starts.get_mut(&structure) {
                        *start = tag;
                        chunk.access.mark_unsaved();
                    }
                }
                if holder.biome_view.is_none()
                    && chunk
                        .get_persisted_status()
                        .is_or_after(ChunkStatus::Biomes)
                {
                    holder.biome_view = Some(Arc::new(biome_only_chunk(
                        &chunk.access,
                        self.level_height,
                        &self.factory,
                    )));
                }
                holder.slot = ChunkSlot::Proto(chunk);
            }
            // a holder that waits to unload could not be saved while its chunk was lent
            self.check_unload_ready(key);
        }
        for (pos, structure, tag) in result.start_updates {
            if let Some(holder) = self.holders.holder_mut(pos.pack()) {
                let access = match &mut holder.slot {
                    ChunkSlot::Proto(p) => Some(&mut p.access),
                    ChunkSlot::Level(l) => Some(&mut l.data.access),
                    _ => None,
                };
                if let Some(access) = access {
                    if let Some(start) = access.structures.starts.get_mut(&structure) {
                        *start = tag;
                        access.mark_unsaved();
                    }
                } else if matches!(holder.slot, ChunkSlot::Loaned) {
                    // lent to a background job: it gets the rewrite when it comes back, readers meanwhile see it in the view
                    if let Some(view) = &mut holder.lent_structures
                        && let Some(start) = Arc::make_mut(view).starts.get_mut(&structure)
                    {
                        *start = tag.clone();
                    }
                    holder.pending_start_updates.push((structure, tag));
                }
            }
        }
        // `OreFeature` wrote into the sections a snapshot shares with its full chunk (raw: no heightmaps, light or unsaved mark)
        for (pos, state) in result.section_writes {
            if let Some(holder) = self
                .holders
                .holder_mut(ChunkPos::pack_xz(pos.x() >> 4, pos.z() >> 4))
                && let ChunkSlot::Level(l) = &mut holder.slot
            {
                let index = self.level_height.section_index(pos.y()) as usize;
                l.data.access.get_section_mut(index).set_block_state(
                    pos.x() & 15,
                    pos.y() & 15,
                    pos.z() & 15,
                    state,
                );
            }
        }
        // `BlendingData.calculateData` ran on the chunk's own data object in vanilla
        for (pos, data) in result.blending_updates {
            if let Some(holder) = self.holders.holder_mut(pos.pack()) {
                match &mut holder.slot {
                    ChunkSlot::Proto(p) => p.access.blending_data = Some(data),
                    ChunkSlot::Level(l) => l.data.access.blending_data = Some(data),
                    _ => {}
                }
            }
        }
        for (key, status) in result.completed {
            self.complete_step(key, status);
        }
        self.process_ready();
    }
}

/// The steps that call `Blender.of(region)`.
fn is_blending_step(task: ChunkStatusTask) -> bool {
    matches!(
        task,
        ChunkStatusTask::GenerateBiomes | ChunkStatusTask::BuildTerrain
    )
}

/// `Blender.of` reads chunks up to `HEIGHT_BLENDING_RANGE_CHUNKS` (7) away and the neighbours of those.
const BLENDING_REACH: i32 = 8;

/// What `Blender.of` reads of a chunk with blending data: the chunk itself while `BlendingData.calculateData` has not run
/// (it reads blocks, heights and biomes), otherwise only the blending data and the statuses.
fn old_snapshot(
    access: &ChunkAccess,
    status: ChunkStatus,
    retrogen: Option<&mc_world::below_zero_retrogen::BelowZeroRetrogen>,
    full: impl FnOnce() -> ProtoChunk,
    level_height: LevelHeight,
    factory: &PalettedContainerFactory,
) -> ProtoChunk {
    let data = access
        .blending_data
        .as_ref()
        .expect("only chunks with blending data are snapshotted");
    if !data.has_calculated_data {
        return full();
    }
    let mut light = ProtoChunk::new(
        access.pos,
        UpgradeData::empty(),
        level_height,
        factory,
        Some(data.clone()),
    );
    light.set_persisted_status(status);
    light.set_below_zero_retrogen(retrogen.cloned());
    light
}

/// [`old_snapshot`] of a proto chunk; `None` without blending data.
pub(super) fn old_view(
    p: &ProtoChunk,
    level_height: LevelHeight,
    factory: &PalettedContainerFactory,
) -> Option<ProtoChunk> {
    p.access.blending_data.as_ref()?;
    Some(old_snapshot(
        &p.access,
        p.get_persisted_status(),
        p.get_below_zero_retrogen(),
        || p.clone(),
        level_height,
        factory,
    ))
}

/// A full chunk as the read-only proto chunk of a region (vanilla's `ImposterProtoChunk`).
fn level_chunk_snapshot(
    l: &crate::world::level::chunk::level_chunk::LevelChunk,
    level_height: LevelHeight,
    factory: &PalettedContainerFactory,
) -> ProtoChunk {
    let mut snapshot = ProtoChunk::new(
        l.pos(),
        mc_world::upgrade_data::UpgradeData::empty(),
        level_height,
        factory,
        None,
    );
    snapshot.access = l.data.access.clone();
    snapshot.set_persisted_status(ChunkStatus::Full);
    snapshot
}

/// `collectPossibleBiomes(region, 1)` and the biome manager of `buildTerrain` read the biomes this many chunks away.
const TERRAIN_BIOME_RADIUS: i32 = 1;

/// A proto chunk with the biome sections of `access` and nothing else.
pub(super) fn biome_only_chunk(
    access: &ChunkAccess,
    level_height: LevelHeight,
    factory: &PalettedContainerFactory,
) -> ProtoChunk {
    let mut view = ProtoChunk::new(
        access.pos,
        UpgradeData::empty(),
        level_height,
        factory,
        None,
    );
    for (dst, src) in view.access.sections.iter_mut().zip(&access.sections) {
        *dst = LevelChunkSection::new(dst.states().clone(), src.biomes().clone());
    }
    view
}
