//! `ChunkHolder` + `GenerationChunkHolder`: per-chunk state of the chunk map.
//!
//! Vanilla expresses the chunk's progress as `CompletableFuture`s per `ChunkStatus` and three promotion futures
//! (full / ticking / entity ticking). Everything lives on the game thread here, so the futures become explicit
//! states ([`FutureState`], [`PromotionFuture`]) and anything that waited on a future registers a [`Waiter`] that the
//! chunk map wakes when the future completes or fails.

use super::chunk_level;
use super::FullChunkStatus;
use crate::world::level::chunk::LevelChunk;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::status::ChunkStatus;
use std::collections::HashSet;

/// Who to wake when a future of this holder completes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Waiter {
    /// A `ChunkGenerationTask` (by id) waiting for its scheduled layer.
    Task(u64),
    /// A pending promotion future (`prepare*Chunk` range wait) of the holder with this key.
    Promotion(i64, PromotionKind),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PromotionKind {
    Full,
    Ticking,
    EntityTicking,
}

/// State of the future of one `ChunkStatus` (`futures[index]`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FutureState {
    /// `null` slot.
    Absent,
    /// Created, not completed.
    Pending,
    /// Completed with the chunk.
    Done,
    /// Completed with `UNLOADED_CHUNK` (kept done in the array only for... never: failed futures are cleared).
    Unloaded,
}

/// `fullChunkFuture` / `tickingChunkFuture` / `entityTickingChunkFuture`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PromotionFuture {
    /// `UNLOADED_LEVEL_CHUNK_FUTURE` (initial value, and the value after a demotion): completed with an error.
    Unloaded,
    /// Created by `prepare*Chunk`, waiting for the chunk range.
    Pending,
    /// Completed with the level chunk.
    Done,
    /// Completed with an error other than the shared constant (a dependency was unloaded).
    Failed,
}

impl PromotionFuture {
    pub fn is_pending(self) -> bool {
        self == PromotionFuture::Pending
    }

    pub fn is_success(self) -> bool {
        self == PromotionFuture::Done
    }
}

/// The chunk object of a holder. Vanilla keeps one `ChunkAccess` per status future; they all refer to the same
/// `ProtoChunk` until `FULL`, where the future values become `ImposterProtoChunk`s wrapping the `LevelChunk`.
#[derive(Default)]
pub enum ChunkSlot {
    #[default]
    None,
    Proto(Box<ProtoChunk>),
    Level(Box<LevelChunk>),
    /// Lent to a worker thread for a generation batch (restored when the batch completes).
    Loaned,
}

pub struct ChunkHolder {
    pos: ChunkPos,
    level_height: LevelHeight,
    /// Unique id (vanilla compares holder identity in `pendingUnloads.remove(pos, holder)`).
    pub serial: u64,

    // GenerationChunkHolder
    highest_allowed_status: Option<ChunkStatus>,
    started_work: Option<ChunkStatus>,
    futures: [FutureState; 10],
    pub(crate) waiters: Vec<(ChunkStatus, Waiter)>,
    pub(crate) task: Option<u64>,
    generation_ref_count: i32,
    /// Status of the chunk object in `slot` (`getPersistedStatus` source; valid while the slot is loaned).
    persisted_status: Option<ChunkStatus>,
    pub slot: ChunkSlot,

    // ChunkHolder
    pub(crate) full_chunk_future: PromotionFuture,
    pub(crate) ticking_chunk_future: PromotionFuture,
    pub(crate) entity_ticking_chunk_future: PromotionFuture,
    pub(crate) promotion_waiters: Vec<Waiter>,
    old_ticket_level: i32,
    ticket_level: i32,
    queue_level: i32,
    has_changed_sections: bool,
    changed_blocks_per_section: Vec<Option<super::fastutil::ShortOpenHashSet>>,
    pub(crate) block_changed_light_section_filter: HashSet<i32>,
    pub(crate) sky_changed_light_section_filter: HashSet<i32>,
    was_accessible_since_last_save: bool,
    /// `pendingFullStateConfirmation`: status to report once the pending promotion succeeds.
    pub(crate) pending_full_state_confirmation: Option<FullChunkStatus>,
    /// `sendSync`: light barriers (`waitForPendingTasks`) that must all finish before the chunk may be sent.
    pub(crate) send_sync_light_barriers: Vec<u64>,
    /// Holders registered to release their throttle slot once the entity ticking future completes.
    pub(crate) release_when_entity_ticking: bool,
}

/// `new ChunkHolder` failure: `GenerationChunkHolder` throws for out-of-bounds positions.
#[derive(Debug)]
pub struct InvalidChunkPos(pub ChunkPos);

impl std::fmt::Display for InvalidChunkPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Trying to create chunk out of reasonable bounds: {}", self.0)
    }
}

impl ChunkHolder {
    pub fn new(pos: ChunkPos, ticket_level: i32, level_height: LevelHeight, serial: u64) -> Result<Self, InvalidChunkPos> {
        if !pos.is_valid() {
            return Err(InvalidChunkPos(pos));
        }
        let old = chunk_level::max_level() + 1;
        let mut holder = Self {
            pos,
            level_height,
            serial,
            highest_allowed_status: None,
            started_work: None,
            futures: [FutureState::Absent; 10],
            waiters: Vec::new(),
            task: None,
            generation_ref_count: 0,
            persisted_status: None,
            slot: ChunkSlot::None,
            full_chunk_future: PromotionFuture::Unloaded,
            ticking_chunk_future: PromotionFuture::Unloaded,
            entity_ticking_chunk_future: PromotionFuture::Unloaded,
            promotion_waiters: Vec::new(),
            old_ticket_level: old,
            ticket_level: old,
            queue_level: old,
            has_changed_sections: false,
            changed_blocks_per_section: vec![None; level_height.sections_count()],
            block_changed_light_section_filter: HashSet::new(),
            sky_changed_light_section_filter: HashSet::new(),
            was_accessible_since_last_save: false,
            pending_full_state_confirmation: None,
            send_sync_light_barriers: Vec::new(),
            release_when_entity_ticking: false,
        };
        holder.set_ticket_level(ticket_level);
        Ok(holder)
    }

    pub fn get_pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn key(&self) -> i64 {
        self.pos.pack()
    }

    // ------------------------------------------------------------------ levels

    pub fn get_ticket_level(&self) -> i32 {
        self.ticket_level
    }

    pub fn get_queue_level(&self) -> i32 {
        self.queue_level
    }

    pub fn set_queue_level(&mut self, queue_level: i32) {
        self.queue_level = queue_level;
    }

    pub fn set_ticket_level(&mut self, ticket_level: i32) {
        self.ticket_level = ticket_level;
    }

    pub fn old_ticket_level(&self) -> i32 {
        self.old_ticket_level
    }

    pub fn set_old_ticket_level(&mut self, level: i32) {
        self.old_ticket_level = level;
    }

    pub fn get_full_status(&self) -> FullChunkStatus {
        chunk_level::full_status(self.ticket_level)
    }

    pub fn was_accessible_since_last_save(&self) -> bool {
        self.was_accessible_since_last_save
    }

    pub fn mark_accessible(&mut self, accessible: bool) {
        self.was_accessible_since_last_save |= accessible;
    }

    pub fn refresh_accessibility(&mut self) {
        self.was_accessible_since_last_save = chunk_level::full_status(self.ticket_level).is_or_after(FullChunkStatus::Full);
    }

    // ------------------------------------------------------------------ generation futures

    pub fn highest_allowed_status(&self) -> Option<ChunkStatus> {
        self.highest_allowed_status
    }

    pub fn set_highest_allowed_status(&mut self, status: Option<ChunkStatus>) {
        self.highest_allowed_status = status;
    }

    /// `isStatusDisallowed(status)`.
    pub fn is_status_disallowed(&self, status: ChunkStatus) -> bool {
        self.highest_allowed_status.is_none_or(|h| status.is_after(h))
    }

    pub fn started_work(&self) -> Option<ChunkStatus> {
        self.started_work
    }

    /// `acquireStatusBump(status)`.
    pub fn acquire_status_bump(&mut self, status: ChunkStatus) -> bool {
        let parent = if status == ChunkStatus::Empty { None } else { Some(status.parent()) };
        let previous = self.started_work;
        if previous == parent {
            self.started_work = Some(status);
            true
        } else if previous.is_some_and(|p| !status.is_after(p)) {
            false
        } else {
            panic!("Unexpected last startedWork status: {previous:?} while trying to start: {status:?}");
        }
    }

    pub fn future_state(&self, status: ChunkStatus) -> FutureState {
        self.futures[status.index() as usize]
    }

    /// `getOrCreateFuture(status)` state: `Unloaded` if disallowed, else the (possibly new) pending state.
    pub fn get_or_create_future(&mut self, status: ChunkStatus) -> FutureState {
        if self.is_status_disallowed(status) {
            return FutureState::Unloaded;
        }
        let slot = &mut self.futures[status.index() as usize];
        if *slot == FutureState::Absent {
            *slot = FutureState::Pending;
        }
        *slot
    }

    /// `completeFuture(status, chunk)`: the chunk object itself is already in `slot`; wakes the waiters.
    pub fn complete_future(&mut self, status: ChunkStatus) -> Vec<Waiter> {
        let index = status.index() as usize;
        if self.futures[index] == FutureState::Done {
            panic!("Trying to complete a future but found it to be completed successfully already");
        }
        self.futures[index] = FutureState::Done;
        self.take_waiters(status)
    }

    fn take_waiters(&mut self, status: ChunkStatus) -> Vec<Waiter> {
        let mut woken = Vec::new();
        self.waiters.retain(|(s, w)| {
            if *s == status {
                woken.push(*w);
                false
            } else {
                true
            }
        });
        woken
    }

    pub fn add_waiter(&mut self, status: ChunkStatus, waiter: Waiter) {
        if !self.waiters.contains(&(status, waiter)) {
            self.waiters.push((status, waiter));
        }
    }

    /// `failAndClearPendingFuturesBetween(fromExclusive, toInclusive)`: returns the waiters to wake.
    pub fn fail_and_clear_pending_futures_between(&mut self, from_exclusive: Option<ChunkStatus>, to_inclusive: ChunkStatus) -> Vec<Waiter> {
        let start = from_exclusive.map_or(0, |s| s.index() + 1);
        let end = to_inclusive.index();
        let mut woken = Vec::new();
        for i in start..=end {
            if self.futures[i as usize] == FutureState::Pending {
                self.futures[i as usize] = FutureState::Absent;
                woken.extend(self.take_waiters(ChunkStatus::ALL[i as usize]));
            }
        }
        woken
    }

    /// `findHighestStatusWithPendingFuture(newStatus)`.
    pub fn find_highest_status_with_pending_future(&self, new_status: Option<ChunkStatus>) -> Option<ChunkStatus> {
        let new_status = new_status?;
        let mut highest = new_status;
        let already_started = self.started_work;
        while already_started.is_none_or(|s| highest.is_after(s)) {
            if self.futures[highest.index() as usize] != FutureState::Absent {
                return Some(highest);
            }
            if highest == ChunkStatus::Empty {
                break;
            }
            highest = highest.parent();
        }
        None
    }

    pub fn increase_generation_ref_count(&mut self) {
        self.generation_ref_count += 1;
    }

    /// `decreaseGenerationRefCount()`.
    pub fn decrease_generation_ref_count(&mut self) {
        self.generation_ref_count -= 1;
        if self.generation_ref_count < 0 {
            panic!("More releases than claims. Count: {}", self.generation_ref_count);
        }
    }

    pub fn generation_ref_count(&self) -> i32 {
        self.generation_ref_count
    }

    /// `isReadyForSaving()` (`saveSync.isDone()`): no generation task claims the holder and no promotion future is pending.
    pub fn is_ready_for_saving(&self) -> bool {
        self.generation_ref_count == 0
            && !self.full_chunk_future.is_pending()
            && !self.ticking_chunk_future.is_pending()
            && !self.entity_ticking_chunk_future.is_pending()
    }

    // ------------------------------------------------------------------ chunk access

    /// The persisted status of the chunk object (`getPersistedStatus`), `None` before the EMPTY future completed.
    pub fn get_persisted_status(&self) -> Option<ChunkStatus> {
        if self.futures[0] == FutureState::Done { self.persisted_status } else { None }
    }

    pub fn set_persisted_status_cache(&mut self, status: Option<ChunkStatus>) {
        self.persisted_status = status;
    }

    /// `getChunkIfPresentUnchecked(status)` as a "done" test: the chunk exists and has completed `status`.
    pub fn has_chunk_unchecked(&self, status: ChunkStatus) -> bool {
        self.futures[status.index() as usize] == FutureState::Done && !matches!(self.slot, ChunkSlot::None)
    }

    pub fn has_chunk(&self, status: ChunkStatus) -> bool {
        !self.is_status_disallowed(status) && self.has_chunk_unchecked(status)
    }

    /// `getLatestStatus()`.
    pub fn get_latest_status(&self) -> Option<ChunkStatus> {
        let status = self.started_work?;
        if self.futures[status.index() as usize] == FutureState::Done { Some(status) } else { Some(status.parent()).filter(|_| status != ChunkStatus::Empty) }
    }

    /// Status of the latest chunk object (`getLatestChunk` helper): `started_work` if done else its parent.
    pub fn latest_chunk_status(&self) -> Option<ChunkStatus> {
        let status = self.started_work?;
        if self.has_chunk_unchecked(status) {
            Some(status)
        } else if status != ChunkStatus::Empty && self.has_chunk_unchecked(status.parent()) {
            Some(status.parent())
        } else {
            None
        }
    }

    pub fn latest_chunk(&self) -> Option<&ChunkSlot> {
        self.latest_chunk_status().map(|_| &self.slot)
    }

    /// `getTickingChunk()`.
    pub fn get_ticking_chunk(&self) -> Option<&LevelChunk> {
        match (&self.slot, self.ticking_chunk_future) {
            (ChunkSlot::Level(chunk), PromotionFuture::Done) => Some(chunk),
            _ => None,
        }
    }

    pub fn get_ticking_chunk_mut(&mut self) -> Option<&mut LevelChunk> {
        match (&mut self.slot, self.ticking_chunk_future) {
            (ChunkSlot::Level(chunk), PromotionFuture::Done) => Some(chunk),
            _ => None,
        }
    }

    /// The `FULL` level chunk (`getFullChunkFuture().getNow(...)`), if the full promotion succeeded.
    pub fn get_full_chunk(&self) -> Option<&LevelChunk> {
        match (&self.slot, self.full_chunk_future) {
            (ChunkSlot::Level(chunk), PromotionFuture::Done) => Some(chunk),
            _ => None,
        }
    }

    pub fn get_full_chunk_mut(&mut self) -> Option<&mut LevelChunk> {
        match (&mut self.slot, self.full_chunk_future) {
            (ChunkSlot::Level(chunk), PromotionFuture::Done) => Some(chunk),
            _ => None,
        }
    }

    /// `getChunkIfPresent(FULL)` as a level chunk (`ServerChunkCache.getChunkNow`).
    pub fn get_chunk_if_present_full(&self) -> Option<&LevelChunk> {
        if self.has_chunk(ChunkStatus::Full) {
            if let ChunkSlot::Level(chunk) = &self.slot {
                return Some(chunk);
            }
        }
        None
    }

    pub fn get_chunk_if_present_full_mut(&mut self) -> Option<&mut LevelChunk> {
        if self.has_chunk(ChunkStatus::Full) {
            if let ChunkSlot::Level(chunk) = &mut self.slot {
                return Some(chunk);
            }
        }
        None
    }

    // ------------------------------------------------------------------ block change broadcasting

    /// `blockChanged(pos)`: returns whether this is the first pending change of the holder.
    pub fn block_changed(&mut self, pos: BlockPos) -> bool {
        if self.get_ticking_chunk().is_none() {
            return false;
        }
        let had_changed_sections = self.has_changed_sections;
        let section_index = self.level_height.section_index(pos.y()) as usize;
        let changed = &mut self.changed_blocks_per_section[section_index];
        if changed.is_none() {
            self.has_changed_sections = true;
            *changed = Some(super::fastutil::ShortOpenHashSet::new());
        }
        let rel = SectionPos::section_relative_pos(&pos);
        changed.as_mut().expect("just created").add(rel);
        !had_changed_sections
    }

    /// `sectionLightChanged(layer, chunkY)`; `min/max_light_section` come from the light engine.
    pub fn section_light_changed(&mut self, sky: bool, chunk_y: i32, min_light_section: i32, max_light_section: i32) -> bool {
        if !self.has_chunk_unchecked(ChunkStatus::InitializeLight) || self.is_status_disallowed(ChunkStatus::InitializeLight) {
            return false;
        }
        match &mut self.slot {
            ChunkSlot::Proto(p) => p.access.mark_unsaved(),
            ChunkSlot::Level(l) => l.data.access.mark_unsaved(),
            _ => {}
        }
        if self.get_ticking_chunk().is_none() {
            return false;
        }
        if chunk_y >= min_light_section && chunk_y <= max_light_section {
            let filter = if sky { &mut self.sky_changed_light_section_filter } else { &mut self.block_changed_light_section_filter };
            let index = chunk_y - min_light_section;
            filter.insert(index)
        } else {
            false
        }
    }

    pub fn has_changes_to_broadcast(&self) -> bool {
        self.has_changed_sections || !self.sky_changed_light_section_filter.is_empty() || !self.block_changed_light_section_filter.is_empty()
    }

    pub fn has_changed_sections(&self) -> bool {
        self.has_changed_sections
    }

    /// Take the changed blocks per section (`(sectionIndex, relative positions)`) and reset the flag; the caller
    /// builds the packets (`broadcastChanges`).
    pub fn take_changed_blocks(&mut self) -> Vec<(usize, Vec<i16>)> {
        let mut out = Vec::new();
        for (index, slot) in self.changed_blocks_per_section.iter_mut().enumerate() {
            if let Some(changed) = slot.take() {
                out.push((index, changed.iter().collect()));
            }
        }
        self.has_changed_sections = false;
        out
    }

    pub fn take_light_filters(&mut self) -> (HashSet<i32>, HashSet<i32>) {
        (std::mem::take(&mut self.sky_changed_light_section_filter), std::mem::take(&mut self.block_changed_light_section_filter))
    }

    pub fn level_height(&self) -> LevelHeight {
        self.level_height
    }
}
