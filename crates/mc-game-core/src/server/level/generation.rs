//! `ChunkGenerationTask` and the `GeneratingChunkMap` side of `ChunkMap`: scheduling status layers through the
//! generation/loading pyramids, applying steps (on the game thread, the worldgen thread, the IO/parse pool or the
//! light engine) and completing the per-status futures.

use super::chunk_holder::{ChunkHolder, ChunkSlot, FutureState, Waiter};
use super::chunk_map::{ChunkMap, MainTask};
use super::chunk_status_tasks::{BatchResult, GenBatch, GenJob, runs_on_worldgen_thread};
use super::world_gen::{GenChunk, LightEvent};
use crate::world::level::chunk::status::{
    ChunkStatusTask, ChunkStep, GENERATION_PYRAMID, LOADING_PYRAMID,
};
use mc_core::pos::{ChunkPos, SectionPos};
use mc_light::LightCompletion;
use mc_world::chunk::{ProtoChunk, StructureData};
use mc_world::status::ChunkStatus;
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
        }
        self.pump_worldgen();
    }

    /// Hand the next queued task slice to the "executor" while the worldgen thread is idle.
    pub(crate) fn pump_worldgen(&mut self) {
        while !self.worldgen_busy {
            // light work waits while a batch has chunks lent out: give it its turn before the next batch starts
            self.run_light_updates();
            let Some((_, tasks)) = self.worldgen_queue.pop() else {
                return;
            };
            for id in tasks {
                self.run_generation_task_slice(id);
            }
        }
    }

    /// One `runUntilWait()` of a task (a worldgen executor slice).
    pub(crate) fn run_generation_task_slice(&mut self, id: u64) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        let waiting_for = self.run_until_wait(id);
        self.flush_batch(id);
        if let Some((key, status)) = waiting_for
            && let Some(h) = self.holders.holder_mut(key)
        {
            h.add_waiter(status, Waiter::Task(id));
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

    /// Send the collected worldgen steps of a task's layer to the worldgen thread.
    fn flush_batch(&mut self, id: u64) {
        let Some(task) = self.tasks.get_mut(&id) else {
            return;
        };
        if task.batch_jobs.is_empty() {
            return;
        }
        let jobs = std::mem::take(&mut task.batch_jobs);
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
                        let Some(holder) = self.holders.holder_mut(key) else {
                            continue;
                        };
                        match std::mem::take(&mut holder.slot) {
                            ChunkSlot::Proto(p) => {
                                chunks.insert(key, GenChunk::Loaned(p));
                                holder.slot = ChunkSlot::Loaned;
                            }
                            ChunkSlot::Level(l) => {
                                let mut snapshot = ProtoChunk::new(
                                    l.pos(),
                                    mc_world::upgrade_data::UpgradeData::empty(),
                                    self.level_height,
                                    &self.factory,
                                    None,
                                );
                                snapshot.access = l.data.access.clone();
                                snapshot.set_persisted_status(ChunkStatus::Full);
                                holder.slot = ChunkSlot::Level(l);
                                chunks.insert(key, GenChunk::Snapshot(Box::new(snapshot)));
                            }
                            other => holder.slot = other,
                        }
                    } else if !structure_views.contains_key(&key)
                        && let Some(holder) = self.holders.holder(key)
                    {
                        match &holder.slot {
                            ChunkSlot::Proto(p) => {
                                structure_views.insert(key, Arc::new(p.access.structures.clone()));
                            }
                            ChunkSlot::Level(l) => {
                                structure_views
                                    .insert(key, Arc::new(l.data.access.structures.clone()));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        let batch_id = self.next_batch_id;
        self.next_batch_id += 1;
        self.in_flight_batch = Some(batch_id);
        self.worldgen_busy = true;
        self.worldgen.submit(
            self.gen_context.clone(),
            GenBatch {
                id: batch_id,
                jobs,
                chunks,
                structure_views,
            },
        );
    }

    /// A worldgen batch finished: take the chunks back and complete the futures of its steps.
    pub(crate) fn on_batch_done(&mut self, result: BatchResult) {
        self.worldgen_busy = false;
        self.in_flight_batch = None;
        // the `getServer().execute(...)` tasks of `updatePOIOnBlockStateChange`, in call order
        for (pos, old_type, new_type) in result.poi_changes {
            self.poi_manager
                .apply_block_state_change(pos, old_type, new_type);
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
        for (key, chunk) in result.chunks {
            if let Some(holder) = self.holders.holder_mut(key) {
                holder.set_persisted_status_cache(Some(chunk.get_persisted_status()));
                holder.slot = ChunkSlot::Proto(chunk);
            }
        }
        for (pos, structure, tag) in result.start_updates {
            if let Some(holder) = self.holders.holder_mut(pos.pack()) {
                let access = match &mut holder.slot {
                    ChunkSlot::Proto(p) => Some(&mut p.access),
                    ChunkSlot::Level(l) => Some(&mut l.data.access),
                    _ => None,
                };
                if let Some(access) = access
                    && let Some(start) = access.structures.starts.get_mut(&structure)
                {
                    *start = tag;
                    access.mark_unsaved();
                }
            }
        }
        for (key, status) in result.completed {
            self.complete_step(key, status);
        }
        self.process_ready();
    }
}
