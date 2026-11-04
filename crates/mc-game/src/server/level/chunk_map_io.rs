//! `ChunkMap` loading (`scheduleChunkLoad`), saving (`save`, `saveChunkIfNeeded`, `saveAllChunks`) and unloading
//! (`processUnloads`, `scheduleUnload`).

use super::chunk_holder::{ChunkHolder, ChunkSlot};
use super::chunk_io::WorkerMessage;
use super::chunk_map::{ChunkMap, ChunkMapEvent};
use crate::world::level::chunk::LevelChunk;
use mc_core::pos::ChunkPos;
use mc_world::chunk::ProtoChunk;
use mc_world::serial::{LoadedChunk, ReadContext, SerializableChunkData};
use mc_world::status::{ChunkStatus, ChunkType};
use mc_world::upgrade_data::UpgradeData;
use std::sync::atomic::Ordering;

const CHUNK_TYPE_REPLACEABLE: i8 = -1;
const CHUNK_TYPE_FULL: i8 = 1;
const CHUNK_SAVED_EAGERLY_PER_TICK: usize = 20;
const EAGER_CHUNK_SAVE_COOLDOWN_IN_MILLIS: i64 = 10000;
const MAX_ACTIVE_CHUNK_WRITES: i32 = 128;

fn millis() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

impl ChunkMap {
    // ------------------------------------------------------------------ loading

    /// `scheduleChunkLoad(pos)`: the read goes to the IO thread; the result continues in [`Self::handle_worker_message`].
    pub(crate) fn start_chunk_load(&mut self, key: i64) {
        self.loading.add(key);
        self.io.read(ChunkPos::unpack(key));
    }

    pub(crate) fn handle_worker_message(&mut self, msg: WorkerMessage) {
        match msg {
            WorkerMessage::ChunkRead { key, result } => match result {
                Ok(None) => self.install_empty_chunk(key),
                Ok(Some(tag)) => {
                    let height = self.level_height;
                    let factory = self.factory.clone();
                    self.pool.submit(move || {
                        let result = SerializableChunkData::parse(height, &factory, &tag);
                        Some(WorkerMessage::ChunkParsed { key, result })
                    });
                }
                Err(e) => self.handle_chunk_load_failure(key, &e.to_string()),
            },
            WorkerMessage::ChunkParsed { key, result } => match result {
                Ok(Some(data)) => self.install_parsed_chunk(key, data),
                Ok(None) => {
                    log::error!("Chunk file at {} is missing level data, skipping", ChunkPos::unpack(key));
                    self.install_empty_chunk(key);
                }
                Err(e) => self.handle_chunk_load_failure(key, &e.to_string()),
            },
            WorkerMessage::BatchDone(result) => self.on_batch_done(*result),
        }
    }

    /// `handleChunkLoadFailure` (non-fatal): report and continue with an empty chunk.
    fn handle_chunk_load_failure(&mut self, key: i64, error: &str) {
        let pos = ChunkPos::unpack(key);
        log::error!("Failed to load chunk {},{}: {error}", pos.x, pos.z);
        self.install_empty_chunk(key);
    }

    /// `createEmptyChunk(pos)`.
    fn install_empty_chunk(&mut self, key: i64) {
        self.chunk_type_cache.insert(key, CHUNK_TYPE_REPLACEABLE);
        let proto = ProtoChunk::new(ChunkPos::unpack(key), UpgradeData::empty(), self.level_height, &self.factory, None);
        self.finish_load(key, ChunkSlot::Proto(Box::new(proto)));
    }

    fn install_parsed_chunk(&mut self, key: i64, data: SerializableChunkData) {
        let pos = ChunkPos::unpack(key);
        let known = self.is_known_structure.clone();
        let ctx = ReadContext { level_height: self.level_height, has_sky_light: self.has_sky_light, factory: &self.factory, is_known_structure: &*known };
        let loaded = data.read(&ctx, pos);
        self.light.load_chunk_light(pos, loaded.light);
        let slot = match loaded.chunk {
            LoadedChunk::Proto(proto) => {
                self.chunk_type_cache.insert(key, CHUNK_TYPE_REPLACEABLE);
                ChunkSlot::Proto(Box::new(proto))
            }
            LoadedChunk::Level(mut chunk) => {
                self.chunk_type_cache.insert(key, CHUNK_TYPE_FULL);
                for tag in loaded.post_load_block_entities {
                    chunk.access.set_block_entity_nbt(tag);
                }
                if !loaded.post_load_entities.is_empty() {
                    self.events.push(ChunkMapEvent::LegacyEntities(pos, loaded.post_load_entities));
                }
                ChunkSlot::Level(Box::new(LevelChunk::from_data(chunk)))
            }
        };
        self.finish_load(key, slot);
    }

    fn finish_load(&mut self, key: i64, slot: ChunkSlot) {
        self.loading.remove(key);
        let Some(holder) = self.holders.holder_mut(key) else { return };
        let status = match &slot {
            ChunkSlot::Proto(p) => p.get_persisted_status(),
            _ => ChunkStatus::Full,
        };
        holder.slot = slot;
        holder.set_persisted_status_cache(Some(status));
        self.complete_step(key, ChunkStatus::Empty);
    }

    // ------------------------------------------------------------------ FULL / ticking completion

    /// Take the chunk object of a holder that reached `FULL` (for `ServerLevel` to turn into a live `LevelChunk`).
    pub fn take_chunk_for_full(&mut self, pos: ChunkPos) -> Option<ChunkSlot> {
        let holder = self.holders.holder_mut(pos.pack())?;
        Some(std::mem::replace(&mut holder.slot, ChunkSlot::Loaned))
    }

    /// The `FULL` step could not run because the chunk is lent out: queue it again behind the current work.
    pub fn requeue_full_step(&mut self, pos: ChunkPos) {
        self.main_tasks.push_back(super::chunk_map::MainTask::FullStep(pos.pack()));
    }

    /// Answer to [`ChunkMapEvent::FullStep`]: the live chunk is built and registered.
    pub fn complete_full_step(&mut self, pos: ChunkPos, mut chunk: Box<LevelChunk>) {
        let key = pos.pack();
        let Some(holder) = self.holders.holder_mut(key) else { return };
        chunk.full_status = holder.get_full_status();
        holder.slot = ChunkSlot::Level(chunk);
        self.complete_step(key, ChunkStatus::Full);
    }

    /// Take the `FULL` chunk out of its holder (e.g. to post-process it with the level).
    pub fn take_level_chunk(&mut self, pos: ChunkPos) -> Option<Box<LevelChunk>> {
        let holder = self.holders.holder_mut(pos.pack())?;
        match std::mem::take(&mut holder.slot) {
            ChunkSlot::Level(l) => Some(l),
            other => {
                holder.slot = other;
                None
            }
        }
    }

    pub fn put_level_chunk(&mut self, pos: ChunkPos, chunk: Box<LevelChunk>) {
        if let Some(holder) = self.holders.holder_mut(pos.pack()) {
            holder.slot = ChunkSlot::Level(chunk);
        }
    }

    /// Answer to [`ChunkMapEvent::StartTicking`]: the chunk was post-processed and started ticking.
    pub fn complete_ticking(&mut self, pos: ChunkPos) {
        let key = pos.pack();
        let still_pending = self.holders.holder(key).is_some_and(|h| h.ticking_chunk_future.is_pending());
        if !still_pending {
            return;
        }
        self.finish_promotion(key, super::chunk_holder::PromotionKind::Ticking, true);
        self.on_chunk_ready_to_send(key);
    }

    // ------------------------------------------------------------------ unloading

    /// `processUnloads(haveTime)`.
    pub fn process_unloads(&mut self, have_time: &dyn Fn() -> bool) {
        let mut dropped = Vec::new();
        self.holders.to_drop.drain_filter_in_order(|key| {
            dropped.push(key);
            true
        });
        for key in dropped {
            if let Some(holder) = self.holders.updating.shift_remove(&key) {
                let serial = holder.serial;
                self.holders.pending_unloads.insert(key, holder);
                self.holders.modified = true;
                self.schedule_unload(key, serial);
            }
        }
        let mut minimal_number_of_chunks_to_process = self.unload_queue.len().saturating_sub(2000);
        while minimal_number_of_chunks_to_process > 0 || have_time() {
            let Some((key, serial)) = self.unload_queue.pop_front() else { break };
            minimal_number_of_chunks_to_process = minimal_number_of_chunks_to_process.saturating_sub(1);
            self.run_unload(key, serial);
        }
        self.save_chunks_eagerly(have_time);
    }

    /// `scheduleUnload(pos, holder)`.
    fn schedule_unload(&mut self, key: i64, serial: u64) {
        self.unload_wanted.insert(key, serial);
        self.check_unload_ready(key);
    }

    /// Queue a wanted unload as soon as the holder is ready for saving (`saveSyncFuture.thenRunAsync(unloadQueue::add)`).
    pub(crate) fn check_unload_ready(&mut self, key: i64) {
        let Some(&serial) = self.unload_wanted.get(&key) else { return };
        let ready = self.holders.pending_unloads.get(&key).is_some_and(|h| h.serial == serial && h.is_ready_for_saving());
        if ready {
            self.unload_wanted.remove(&key);
            self.unload_queue.push_back((key, serial));
        }
    }

    fn run_unload(&mut self, key: i64, serial: u64) {
        let Some(holder) = self.holders.pending_unloads.get(&key) else { return };
        if holder.serial != serial {
            return;
        }
        if !holder.is_ready_for_saving() {
            // A new save dependency appeared meanwhile: wait for it.
            self.schedule_unload(key, serial);
            return;
        }
        let mut holder = self.holders.pending_unloads.shift_remove(&key).expect("present");
        let pos = holder.get_pos();
        if holder.latest_chunk_status().is_some() {
            let saved = match &mut holder.slot {
                ChunkSlot::Level(chunk) => {
                    chunk.data.set_loaded(false);
                    self.save_slot(pos, &mut holder.slot)
                }
                slot @ ChunkSlot::Proto(_) => self.save_slot(pos, slot),
                _ => false,
            };
            let _ = saved;
            if let ChunkSlot::Level(chunk) = std::mem::take(&mut holder.slot) {
                self.events.push(ChunkMapEvent::Unload(chunk));
            }
            self.light.update_chunk_status(pos);
            self.next_chunk_save_time.remove(&key);
        }
    }

    fn save_chunks_eagerly(&mut self, have_time: &dyn Fn() -> bool) {
        let now = millis();
        let mut eagerly_saved_count = 0;
        let keys: Vec<i64> = self.chunks_to_eagerly_save.iter().copied().collect();
        for key in keys {
            if !(eagerly_saved_count < CHUNK_SAVED_EAGERLY_PER_TICK && self.active_chunk_writes.load(Ordering::Acquire) < MAX_ACTIVE_CHUNK_WRITES && have_time()) {
                break;
            }
            let unsaved = self.holders.visible_holder(key).and_then(|h| match &h.slot {
                ChunkSlot::Level(l) if h.latest_chunk_status().is_some() => Some(l.data.access.is_unsaved()),
                ChunkSlot::Proto(p) if h.latest_chunk_status().is_some() => Some(p.access.is_unsaved()),
                _ => None,
            });
            if unsaved != Some(true) {
                self.chunks_to_eagerly_save.shift_remove(&key);
            } else if self.save_chunk_if_needed(key, now) {
                eagerly_saved_count += 1;
                self.chunks_to_eagerly_save.shift_remove(&key);
            }
        }
    }

    /// `setChunkUnsaved(pos)` (`WorldGenContext.unsavedListener`).
    pub fn set_chunk_unsaved(&mut self, pos: ChunkPos) {
        self.chunks_to_eagerly_save.insert(pos.pack());
    }

    // ------------------------------------------------------------------ saving

    /// `saveChunkIfNeeded(chunk, now)`.
    pub(crate) fn save_chunk_if_needed(&mut self, key: i64, now: i64) -> bool {
        let Some(holder) = self.holders.visible_holder(key) else { return false };
        if !(holder.was_accessible_since_last_save() && holder.is_ready_for_saving()) {
            return false;
        }
        // Only full chunks (imposter or level chunks) are saved here.
        let (is_full, is_unsaved) = match &holder.slot {
            ChunkSlot::Level(l) if holder.latest_chunk_status().is_some() => (true, l.data.access.is_unsaved()),
            _ => (false, false),
        };
        if !is_full || !is_unsaved {
            return false;
        }
        let next_save_time = self.next_chunk_save_time.get(&key).copied().unwrap_or(-1);
        if now < next_save_time {
            return false;
        }
        let pos = ChunkPos::unpack(key);
        let mut slot = std::mem::take(&mut self.holders.holder_mut(key).expect("holder").slot);
        let saved = self.save_slot(pos, &mut slot);
        let holder = self.holders.holder_mut(key).expect("holder");
        holder.slot = slot;
        holder.refresh_accessibility();
        if saved {
            self.next_chunk_save_time.insert(key, now + EAGER_CHUNK_SAVE_COOLDOWN_IN_MILLIS);
        }
        saved
    }

    /// `save(chunk)`: queue the chunk's NBT for writing. `false` if there was nothing to save.
    pub fn save_slot(&mut self, pos: ChunkPos, slot: &mut ChunkSlot) -> bool {
        let (access_unsaved, status) = match slot {
            ChunkSlot::Level(l) => (l.data.access.try_mark_saved(), ChunkStatus::Full),
            ChunkSlot::Proto(p) => (p.access.try_mark_saved(), p.get_persisted_status()),
            _ => return false,
        };
        if !access_unsaved {
            return false;
        }
        if status.chunk_type() != ChunkType::LevelChunk {
            if self.is_existing_chunk_full(pos) {
                return false;
            }
            let any_valid = match slot {
                ChunkSlot::Proto(p) => p.access.structures.starts.values().any(|s| s.get_string("id").is_some_and(|id| id != "INVALID")),
                _ => false,
            };
            if status == ChunkStatus::Empty && !any_valid {
                return false;
            }
        }
        let game_time = self.game_time;
        let light_fn = self.light.engine().chunk_light(pos);
        let data = match slot {
            ChunkSlot::Level(l) => {
                let block_entities = l.block_entity_nbt_for_saving();
                SerializableChunkData::from_level_chunk(&l.data, game_time, &light_fn, block_entities)
            }
            ChunkSlot::Proto(p) => {
                let block_entities = p.access.block_entities.values().cloned().collect();
                SerializableChunkData::from_proto_chunk(p, game_time, &light_fn, block_entities)
            }
            _ => return false,
        };
        self.active_chunk_writes.fetch_add(1, Ordering::AcqRel);
        let factory = self.factory.clone();
        let io = self.io.handle();
        let active = self.active_chunk_writes.clone();
        let encoding = self.encoding_chunks.clone();
        encoding.fetch_add(1, Ordering::AcqRel);
        self.pool.submit(move || {
            match data.write(&factory) {
                Ok(tag) => io.write(pos, Some(tag), Some(active)),
                Err(e) => {
                    log::error!("Failed to save chunk {},{}: {e}", pos.x, pos.z);
                    active.fetch_sub(1, Ordering::AcqRel);
                }
            }
            encoding.fetch_sub(1, Ordering::AcqRel);
            None
        });
        self.chunk_type_cache.insert(pos.pack(), if status.chunk_type() == ChunkType::ProtoChunk { CHUNK_TYPE_REPLACEABLE } else { CHUNK_TYPE_FULL });
        true
    }

    /// `isExistingChunkFull(pos)`.
    fn is_existing_chunk_full(&mut self, pos: ChunkPos) -> bool {
        let key = pos.pack();
        if let Some(&cached) = self.chunk_type_cache.get(&key) {
            return cached == CHUNK_TYPE_FULL;
        }
        match self.io.read_blocking(pos) {
            Ok(None) => {
                self.chunk_type_cache.insert(key, CHUNK_TYPE_REPLACEABLE);
                false
            }
            Err(e) => {
                log::error!("Failed to read chunk {pos}: {e}");
                self.chunk_type_cache.insert(key, CHUNK_TYPE_REPLACEABLE);
                false
            }
            Ok(Some(tag)) => {
                let full = SerializableChunkData::get_chunk_status_from_tag(Some(&tag)).chunk_type() == ChunkType::LevelChunk;
                self.chunk_type_cache.insert(key, if full { CHUNK_TYPE_FULL } else { CHUNK_TYPE_REPLACEABLE });
                full
            }
        }
    }

    /// First half of `saveAllChunks(true)`: the visible holders that were accessible since the last save.
    pub fn collect_chunks_to_save(&mut self) -> Vec<i64> {
        let keys: Vec<i64> = self.holders.visible.iter().copied().collect();
        let mut out = Vec::new();
        for key in keys {
            if let Some(h) = self.holders.holder_mut(key) {
                if h.was_accessible_since_last_save() {
                    h.refresh_accessibility();
                    out.push(key);
                }
            }
        }
        out
    }

    /// One round of `saveAllChunks(true)` for a chunk that is ready for saving: whether it was saved.
    pub fn save_full_chunk_for_flush(&mut self, key: i64) -> bool {
        let Some(holder) = self.holders.holder_mut(key) else { return false };
        if holder.latest_chunk_status().is_none() || !matches!(holder.slot, ChunkSlot::Level(_)) {
            return false;
        }
        let pos = holder.get_pos();
        let mut slot = std::mem::take(&mut holder.slot);
        let saved = self.save_slot(pos, &mut slot);
        self.holders.holder_mut(key).expect("holder").slot = slot;
        saved
    }

    pub fn is_ready_for_saving(&self, key: i64) -> bool {
        self.holders.holder(key).is_none_or(ChunkHolder::is_ready_for_saving)
    }

    /// `saveAllChunks(false)`: periodic autosave.
    pub fn save_all_chunks_periodic(&mut self) {
        self.next_chunk_save_time.clear();
        let now = millis();
        let keys: Vec<i64> = self.holders.visible.iter().copied().collect();
        for key in keys {
            self.save_chunk_if_needed(key, now);
        }
    }

    /// `synchronize(flush).join()`.
    pub fn synchronize(&self, flush: bool) {
        // Queued saves are encoded on the pool first; wait until all of them reached the IO thread.
        while self.encoding_chunks.load(Ordering::Acquire) > 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if let Err(e) = self.io.synchronize(flush) {
            log::warn!("Failed to synchronize chunks: {e}");
        }
    }

    /// `close()`: stop the worker threads (the region files are flushed and closed).
    pub fn close(&mut self) {
        self.worldgen.close();
        self.pool.close();
        if let Err(e) = self.io.close() {
            log::error!("Failed to close storage: {e}");
        }
    }
}
