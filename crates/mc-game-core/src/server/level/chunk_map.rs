//! `ChunkMap`: the chunk holders of one level, their ticket levels, the generation pipeline, loading, saving and
//! unloading, and which players track which chunks.
//!
//! The vanilla future graph is explicit state here (see [`super::chunk_holder`]); `ChunkMap` is driven by the game
//! thread through [`ChunkMap::poll_task`] and never blocks on workers. Everything that needs the live level (building
//! `LevelChunk`s, starting ticking, entity visibility, unloading chunks) is reported as a [`ChunkMapEvent`] that
//! `ServerLevel` handles right after the chunk source call and answers with the `complete_*` methods.

use super::chunk_holder::{ChunkHolder, ChunkSlot, FutureState, PromotionFuture, PromotionKind, Waiter};
use super::chunk_io::{RegionIo, WorkerMessage, WorkerPool, WorldgenThread};
use super::chunk_level;
use super::chunk_task_queue::ChunkTaskPriorityQueue;
use super::chunk_tracking_view::ChunkTrackingView;
use super::distance_manager::{DistanceHost, DistanceManager, DistanceTask};
use super::fastutil::LongOpenHashSet;
use super::generation::ChunkGenerationTask;
use super::player_map::PlayerMap;
use super::world_gen::WorldGenContext;
use super::{FullChunkStatus, Ticket, TicketType};
use crate::server::network::ConnectionSender;
use crate::world::entity::EntityId;
use crate::world::entity::ai::village::poi::PoiManager;
use crate::world::level::DimensionKey;
use crate::world::level::chunk::LevelChunk;
use crate::world::level::ticket_storage::TicketStorage;
use indexmap::{IndexMap, IndexSet};
use mc_core::phys::Vec3;
use mc_core::pos::{ChunkPos, SectionPos};
use mc_light::ThreadedLevelLightEngine;
use mc_world::chunk::LevelHeight;
use mc_world::palette::PalettedContainerFactory;
use mc_world::region::RegionStorageInfo;
use mc_world::status::ChunkStatus;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicI32;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use uuid::Uuid;

pub const MIN_VIEW_DISTANCE: i32 = 2;
pub const MAX_VIEW_DISTANCE: i32 = 32;

/// `ChunkMap.FORCED_TICKET_LEVEL`.
pub const FORCED_TICKET_LEVEL: i32 = chunk_level::ENTITY_TICKING_LEVEL;

/// The holder maps (`updatingChunkMap`, `pendingUnloads`, `toDrop`, ...): the host of the loading tracker.
pub struct Holders {
    /// `updatingChunkMap` (insertion ordered like `Long2ObjectLinkedOpenHashMap`).
    pub updating: IndexMap<i64, ChunkHolder>,
    /// `pendingUnloads`.
    pub pending_unloads: IndexMap<i64, ChunkHolder>,
    /// Keys of `visibleChunkMap` (a copy of `updating`'s keys taken at the last promotion).
    pub visible: IndexSet<i64>,
    pub to_drop: LongOpenHashSet,
    pub modified: bool,
    pub next_serial: u64,
    pub level_height: LevelHeight,
}

impl Holders {
    pub fn holder(&self, key: i64) -> Option<&ChunkHolder> {
        self.updating.get(&key).or_else(|| self.pending_unloads.get(&key))
    }

    pub fn holder_mut(&mut self, key: i64) -> Option<&mut ChunkHolder> {
        if self.updating.contains_key(&key) { self.updating.get_mut(&key) } else { self.pending_unloads.get_mut(&key) }
    }

    /// `getVisibleChunkIfPresent(key)`.
    pub fn visible_holder(&self, key: i64) -> Option<&ChunkHolder> {
        if self.visible.contains(&key) { self.holder(key) } else { None }
    }

    pub fn visible_holder_mut(&mut self, key: i64) -> Option<&mut ChunkHolder> {
        if self.visible.contains(&key) { self.holder_mut(key) } else { None }
    }
}

impl DistanceHost for Holders {
    fn is_chunk_to_remove(&self, node: i64) -> bool {
        self.to_drop.contains(node)
    }

    fn holder_ticket_level(&self, node: i64) -> Option<i32> {
        self.updating.get(&node).map(ChunkHolder::get_ticket_level)
    }

    fn update_chunk_scheduling(&mut self, node: i64, level: i32, old_level: i32) -> bool {
        if !chunk_level::is_loaded(old_level) && !chunk_level::is_loaded(level) {
            return self.updating.contains_key(&node);
        }
        let exists = self.updating.contains_key(&node);
        if exists {
            self.updating.get_mut(&node).expect("exists").set_ticket_level(level);
            if !chunk_level::is_loaded(level) {
                self.to_drop.add(node);
            } else {
                self.to_drop.remove(node);
            }
        }
        if chunk_level::is_loaded(level) && !exists {
            let holder = match self.pending_unloads.shift_remove(&node) {
                Some(mut holder) => {
                    holder.set_ticket_level(level);
                    holder
                }
                None => {
                    let serial = self.next_serial;
                    self.next_serial += 1;
                    ChunkHolder::new(ChunkPos::unpack(node), level, self.level_height, serial).unwrap_or_else(|e| panic!("{e}"))
                }
            };
            self.updating.insert(node, holder);
            self.modified = true;
            return true;
        }
        exists
    }
}

/// Work that runs "on the main thread" in vanilla (`mainThreadExecutor.execute`), queued in order.
pub enum MainTask {
    Worker(WorkerMessage),
    Distance(DistanceTask),
    /// `ChunkStatusTasks.full` body.
    FullStep(i64),
    /// `prepareTickingChunk` continuation (post-process generation, start ticking).
    TickingReady(i64),
    /// `onFullChunkStatusChange(pos, status)` after a promotion succeeded.
    StatusChange(ChunkPos, FullChunkStatus),
    /// Wake a generation task (`future.thenRun(runGenerationTask)`).
    RunTask(u64),
}

/// What `ServerLevel` has to do with the live level.
pub enum ChunkMapEvent {
    /// Build the `LevelChunk` of a chunk reaching `FULL` (answer with [`ChunkMap::complete_full_step`]).
    FullStep(ChunkPos),
    /// Post-process generation and start ticking (answer with [`ChunkMap::complete_ticking`]).
    StartTicking(ChunkPos),
    /// `entityManager.updateChunkStatus(pos, status)`.
    FullStatusChanged(ChunkPos, FullChunkStatus),
    /// `level.unload(chunk)`: the chunk left the map after saving.
    Unload(Box<LevelChunk>),
    /// Legacy entity tags found in a full chunk's NBT (`postLoad` entities).
    LegacyEntities(ChunkPos, Vec<mc_nbt::CompoundTag>),
}

/// A player registered with the chunk map.
pub struct PlayerTrackingInfo {
    pub entity_id: EntityId,
    pub uuid: Uuid,
    pub connection: ConnectionSender,
    pub sink: Box<dyn super::chunk_send_sink::ChunkSendSink>,
    pub position: Vec3,
    pub requested_view_distance: i32,
    pub spectator: bool,
    pub alive: bool,
}

/// Per-tick/per-move state of a player (`ServerChunkCache.move(player)`).
#[derive(Clone, Copy, Debug)]
pub struct PlayerTrackingUpdate {
    pub entity_id: EntityId,
    pub position: Vec3,
    pub requested_view_distance: i32,
    pub spectator: bool,
    pub alive: bool,
}

pub struct TrackedPlayer {
    pub info: PlayerTrackingInfo,
    pub view: ChunkTrackingView,
    pub last_section_pos: SectionPos,
}

/// Settings the level keeps in sync.
#[derive(Clone, Copy, Debug)]
pub struct ChunkMapSettings {
    /// `GameRules.SPECTATORS_GENERATE_CHUNKS`.
    pub spectators_generate_chunks: bool,
    /// `ServerLevel.noSave`.
    pub no_save: bool,
}

/// Construction parameters (what vanilla reads from `ServerLevel`/`MinecraftServer`).
pub struct ChunkMapParams {
    pub dimension: DimensionKey,
    pub level_id: String,
    pub region_dir: PathBuf,
    /// `<dimension>/poi` (`PoiManager` storage folder).
    pub poi_dir: PathBuf,
    pub level_height: LevelHeight,
    pub has_sky_light: bool,
    pub factory: Arc<PalettedContainerFactory>,
    pub gen_context: Arc<WorldGenContext>,
    pub is_known_structure: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub sync_writes: bool,
    pub view_distance: i32,
    pub tickets: TicketStorage,
}

pub struct ChunkMap {
    pub holders: Holders,
    pub distance_manager: DistanceManager,
    pub tickets: TicketStorage,
    pub dimension: DimensionKey,
    pub level_height: LevelHeight,
    pub has_sky_light: bool,
    pub factory: Arc<PalettedContainerFactory>,
    pub gen_context: Arc<WorldGenContext>,
    pub(crate) is_known_structure: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub light: ThreadedLevelLightEngine,
    pub settings: ChunkMapSettings,
    /// `level.getGameTime()` for chunk saving (kept in sync by the level).
    pub game_time: i64,
    /// The level's scheduled tick containers (the pending ticks of loaded chunks live there, see `level_chunk_ops`).
    pub tick_containers: Option<super::level_tick_state::SharedTickContainers>,

    // threads
    pub(crate) io: RegionIo,
    pub(crate) pool: WorkerPool,
    pub(crate) worldgen: WorldgenThread,
    pub(crate) results: Receiver<WorkerMessage>,
    pub(crate) result_tx: Sender<WorkerMessage>,
    pub(crate) worldgen_busy: bool,
    /// Holders whose chunk just became ready to send (`ServerChunkCache.onChunkReadyToSend` input).
    pub chunk_ready_to_send: Vec<i64>,

    // pipeline
    pub(crate) tasks: HashMap<u64, ChunkGenerationTask>,
    pub(crate) next_task_id: u64,
    pub(crate) pending_generation_tasks: Vec<u64>,
    pub(crate) worldgen_queue: ChunkTaskPriorityQueue<u64>,
    pub(crate) ready: VecDeque<(i64, Waiter)>,
    pub main_tasks: VecDeque<MainTask>,
    pub events: Vec<ChunkMapEvent>,
    pub(crate) next_batch_id: u64,
    pub(crate) in_flight_batch: Option<u64>,
    /// Chunks whose EMPTY load is waiting for the pool/IO.
    pub(crate) loading: LongOpenHashSet,
    /// Per-light-barrier: holders whose `sendSync` waits for it.
    /// Light barriers handed out by `waitForLightBeforeSending` that the light thread has not run yet.
    pub(crate) light_barrier_pending: std::collections::HashSet<u64>,

    // saving
    pub(crate) chunk_type_cache: HashMap<i64, i8>,
    pub(crate) next_chunk_save_time: HashMap<i64, i64>,
    pub(crate) chunks_to_eagerly_save: IndexSet<i64>,
    pub(crate) unload_queue: VecDeque<(i64, u64)>,
    pub(crate) unload_wanted: HashMap<i64, u64>,
    pub(crate) active_chunk_writes: Arc<AtomicI32>,
    /// Save jobs (NBT encoding on the pool) not finished yet.
    pub(crate) encoding_chunks: Arc<AtomicI32>,
    pub(crate) storage_info: RegionStorageInfo,
    /// `poiManager`.
    pub poi_manager: PoiManager,

    // players
    pub player_map: PlayerMap,
    pub players: IndexMap<EntityId, TrackedPlayer>,
    pub(crate) server_view_distance: i32,
}

impl ChunkMap {
    pub fn new(params: ChunkMapParams) -> Self {
        let ChunkMapParams { dimension, level_id, region_dir, poi_dir, level_height, has_sky_light, factory, gen_context, is_known_structure, sync_writes, view_distance, tickets } = params;
        let (result_tx, results) = channel();
        let storage_info = RegionStorageInfo::new(level_id.clone(), dimension.clone(), "chunk");
        let poi_manager = PoiManager::new(RegionStorageInfo::new(level_id, dimension.clone(), "poi"), poi_dir, sync_writes, level_height);
        let io = RegionIo::new(storage_info.clone(), region_dir, sync_writes, result_tx.clone());
        let pool = WorkerPool::new("Chunk-Pool", std::thread::available_parallelism().map_or(2, |n| n.get().min(4)), result_tx.clone());
        let worldgen = WorldgenThread::new(result_tx.clone());
        let mut map = Self {
            holders: Holders {
                updating: IndexMap::new(),
                pending_unloads: IndexMap::new(),
                visible: IndexSet::new(),
                to_drop: LongOpenHashSet::new(),
                modified: false,
                next_serial: 1,
                level_height,
            },
            distance_manager: DistanceManager::new(),
            tickets,
            dimension,
            level_height,
            has_sky_light,
            factory,
            gen_context,
            is_known_structure,
            light: ThreadedLevelLightEngine::new(level_height, has_sky_light, super::chunk_task_queue::priority_level_count()),
            settings: ChunkMapSettings { spectators_generate_chunks: false, no_save: false },
            game_time: 0,
            tick_containers: None,
            io,
            pool,
            worldgen,
            results,
            result_tx,
            worldgen_busy: false,
            chunk_ready_to_send: Vec::new(),
            tasks: HashMap::new(),
            next_task_id: 1,
            pending_generation_tasks: Vec::new(),
            worldgen_queue: ChunkTaskPriorityQueue::new(super::chunk_task_queue::priority_level_count()),
            ready: VecDeque::new(),
            main_tasks: VecDeque::new(),
            events: Vec::new(),
            next_batch_id: 1,
            in_flight_batch: None,
            loading: LongOpenHashSet::new(),
            light_barrier_pending: std::collections::HashSet::new(),
            chunk_type_cache: HashMap::new(),
            next_chunk_save_time: HashMap::new(),
            chunks_to_eagerly_save: IndexSet::new(),
            unload_queue: VecDeque::new(),
            unload_wanted: HashMap::new(),
            active_chunk_writes: Arc::new(AtomicI32::new(0)),
            encoding_chunks: Arc::new(AtomicI32::new(0)),
            storage_info,
            poi_manager,
            player_map: PlayerMap::new(),
            players: IndexMap::new(),
            server_view_distance: 0,
        };
        map.set_server_view_distance(view_distance);
        map
    }

    // ------------------------------------------------------------------ lookups

    pub fn get_updating_chunk_if_present(&self, key: i64) -> Option<&ChunkHolder> {
        self.holders.updating.get(&key)
    }

    /// `getVisibleChunkIfPresent(key)`.
    pub fn get_visible_chunk_if_present(&self, key: i64) -> Option<&ChunkHolder> {
        self.holders.visible_holder(key)
    }

    /// `getLatestStatus(key)`.
    pub fn get_latest_status(&self, key: i64) -> Option<ChunkStatus> {
        self.get_visible_chunk_if_present(key).and_then(ChunkHolder::get_latest_status)
    }

    /// `getChunkQueueLevel(pos)`.
    pub fn get_chunk_queue_level(&self, pos: i64) -> i32 {
        let max = super::chunk_task_queue::priority_level_count() - 1;
        self.get_visible_chunk_if_present(pos).map_or(max, |c| c.get_queue_level().min(max))
    }

    /// `size()`.
    pub fn size(&self) -> usize {
        self.holders.visible.len()
    }

    /// `allChunksWithAtLeastStatus(status)`: keys of visible holders whose ticket level is at most the level of `status`.
    pub fn all_chunks_with_at_least_status(&self, status: ChunkStatus) -> Vec<i64> {
        let level = chunk_level::by_status(status);
        self.holders
            .visible
            .iter()
            .copied()
            .filter(|k| self.holders.holder(*k).is_some_and(|h| h.get_ticket_level() <= level))
            .collect()
    }

    pub fn pending_task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn worldgen_busy(&self) -> bool {
        self.worldgen_busy
    }

    pub fn loading_count(&self) -> usize {
        self.loading.len()
    }

    pub fn unload_queue_len(&self) -> usize {
        self.unload_queue.len()
    }

    /// `hasWork()`.
    pub fn has_work(&self) -> bool {
        self.light.engine().has_light_work()
            || self.light.dispatcher_has_work()
            || !self.holders.pending_unloads.is_empty()
            || !self.holders.updating.is_empty()
            || !self.holders.to_drop.is_empty()
            || !self.unload_queue.is_empty()
            || self.worldgen_busy
            || self.worldgen_queue.has_work()
            || !self.pending_generation_tasks.is_empty()
            || self.poi_manager.has_work()
            || !self.main_tasks.is_empty()
            || self.distance_manager_has_tickets()
    }

    fn distance_manager_has_tickets(&self) -> bool {
        self.tickets.has_tickets()
    }

    /// `promoteChunkMap()`.
    pub fn promote_chunk_map(&mut self) -> bool {
        if !self.holders.modified {
            return false;
        }
        self.holders.visible = self.holders.updating.keys().copied().collect();
        self.holders.modified = false;
        true
    }

    // ------------------------------------------------------------------ distance manager

    /// `DistanceManager.runAllUpdates(chunkMap)`.
    pub fn run_all_updates(&mut self) -> bool {
        let updated = self.distance_manager.run_updates(&mut self.holders, &mut self.tickets);
        self.queue_distance_tasks();
        if !self.distance_manager.chunks_to_update_futures.is_empty() {
            let keys = self.distance_manager.chunks_to_update_futures.drain();
            for key in &keys {
                self.update_highest_allowed_status(*key);
            }
            for key in &keys {
                self.update_futures(*key);
            }
            self.queue_distance_tasks();
            true
        } else {
            if !self.distance_manager.tickets_to_release.is_empty() {
                let positions: Vec<i64> = self.distance_manager.tickets_to_release.iter().collect();
                for pos in positions {
                    if self.tickets.get_tickets(pos).iter().any(|t| t.get_type() == TicketType::PLAYER_LOADING) {
                        let Some(holder) = self.holders.updating.get_mut(&pos) else { panic!("IllegalStateException") };
                        // `entityTickingChunkFuture.thenAccept(c -> execute(() -> release(pos)))`
                        if holder.entity_ticking_chunk_future.is_pending() {
                            holder.release_when_entity_ticking = true;
                        } else {
                            self.main_tasks.push_back(MainTask::Distance(DistanceTask::ReleaseSlot { pos }));
                        }
                    }
                }
                self.distance_manager.tickets_to_release.clear();
            }
            updated
        }
    }

    fn queue_distance_tasks(&mut self) {
        for t in std::mem::take(&mut self.distance_manager.main_tasks) {
            self.main_tasks.push_back(MainTask::Distance(t));
        }
    }

    /// `ChunkHolder.updateHighestAllowedStatus(scheduler)`.
    fn update_highest_allowed_status(&mut self, key: i64) {
        let Some(holder) = self.holders.holder_mut(key) else { return };
        let old_status = holder.highest_allowed_status();
        let new_status = chunk_level::generation_status(holder.get_ticket_level());
        holder.set_highest_allowed_status(new_status);
        let status_dropped = old_status.is_some_and(|old| new_status.is_none_or(|n| n.is_before(old)));
        if status_dropped {
            let woken = holder.fail_and_clear_pending_futures_between(new_status, old_status.expect("dropped"));
            let has_task = holder.task.is_some();
            let pending_status = holder.find_highest_status_with_pending_future(new_status);
            self.wake(key, woken);
            if has_task {
                self.reschedule_chunk_task(key, pending_status);
            }
        }
    }

    pub(crate) fn wake(&mut self, key: i64, waiters: Vec<Waiter>) {
        for w in waiters {
            self.ready.push_back((key, w));
        }
    }

    /// `ChunkHolder.updateFutures(scheduler, mainThreadExecutor)`.
    fn update_futures(&mut self, key: i64) {
        let Some(holder) = self.holders.holder_mut(key) else { return };
        let old_full = chunk_level::full_status(holder.old_ticket_level());
        let new_full = chunk_level::full_status(holder.get_ticket_level());
        let was_accessible = old_full.is_or_after(FullChunkStatus::Full);
        let is_accessible = new_full.is_or_after(FullChunkStatus::Full);
        holder.mark_accessible(is_accessible);

        if !was_accessible && is_accessible {
            self.prepare_promotion(key, PromotionKind::Full);
        }
        let holder = self.holders.holder_mut(key).expect("holder");
        if was_accessible && !is_accessible {
            holder.full_chunk_future = PromotionFuture::Unloaded;
            let _w = std::mem::take(&mut holder.promotion_waiters);
        }
        let was_ticking = old_full.is_or_after(FullChunkStatus::BlockTicking);
        let is_ticking = new_full.is_or_after(FullChunkStatus::BlockTicking);
        if !was_ticking && is_ticking {
            self.prepare_promotion(key, PromotionKind::Ticking);
        }
        let holder = self.holders.holder_mut(key).expect("holder");
        if was_ticking && !is_ticking {
            holder.ticking_chunk_future = PromotionFuture::Unloaded;
        }
        let was_entity_ticking = old_full.is_or_after(FullChunkStatus::EntityTicking);
        let is_entity_ticking = new_full.is_or_after(FullChunkStatus::EntityTicking);
        if !was_entity_ticking && is_entity_ticking {
            assert!(holder.entity_ticking_chunk_future == PromotionFuture::Unloaded, "IllegalStateException");
            self.prepare_promotion(key, PromotionKind::EntityTicking);
        }
        let holder = self.holders.holder_mut(key).expect("holder");
        if was_entity_ticking && !is_entity_ticking {
            let release = std::mem::take(&mut holder.release_when_entity_ticking);
            holder.entity_ticking_chunk_future = PromotionFuture::Unloaded;
            if release {
                self.main_tasks.push_back(MainTask::Distance(DistanceTask::ReleaseSlot { pos: key }));
            }
        }
        let holder = self.holders.holder_mut(key).expect("holder");
        if !new_full.is_or_after(old_full) {
            // `demoteFullChunk`: cancel a pending confirmation and report immediately.
            holder.pending_full_state_confirmation = None;
            self.events.push(ChunkMapEvent::FullStatusChanged(ChunkPos::unpack(key), new_full));
        }
        let holder = self.holders.holder_mut(key).expect("holder");
        let ticket_level = holder.get_ticket_level();
        let old_queue = holder.get_queue_level();
        holder.set_old_ticket_level(ticket_level);
        // `onLevelChange` -> `worldgenTaskDispatcher.onLevelChange(pos, oldLevel, newLevel, setQueueLevel)`.
        holder.set_queue_level(ticket_level);
        self.worldgen_queue.resort_chunk_tasks(old_queue, key, ticket_level);
        self.check_unload_ready(key);
    }

    // ------------------------------------------------------------------ promotions (full / ticking / entity ticking)

    fn promotion_params(kind: PromotionKind) -> i32 {
        match kind {
            PromotionKind::Full | PromotionKind::Ticking => 1,
            PromotionKind::EntityTicking => 2,
        }
    }

    fn promotion_status(kind: PromotionKind, distance: i32) -> ChunkStatus {
        match kind {
            PromotionKind::Full => chunk_level::get_status_around_full_chunk(distance),
            _ => ChunkStatus::Full,
        }
    }

    /// `prepareAccessibleChunk` / `prepareTickingChunk` / `prepareEntityTickingChunk` + `scheduleFullChunkPromotion`.
    fn prepare_promotion(&mut self, key: i64, kind: PromotionKind) {
        let status = match kind {
            PromotionKind::Full => FullChunkStatus::Full,
            PromotionKind::Ticking => FullChunkStatus::BlockTicking,
            PromotionKind::EntityTicking => FullChunkStatus::EntityTicking,
        };
        {
            let holder = self.holders.holder_mut(key).expect("holder");
            holder.pending_full_state_confirmation = Some(status);
            match kind {
                PromotionKind::Full => holder.full_chunk_future = PromotionFuture::Pending,
                PromotionKind::Ticking => holder.ticking_chunk_future = PromotionFuture::Pending,
                PromotionKind::EntityTicking => holder.entity_ticking_chunk_future = PromotionFuture::Pending,
            }
        }
        // `getChunkRangeFuture`: schedule every chunk of the range.
        let range = Self::promotion_params(kind);
        let center = ChunkPos::unpack(key);
        let mut deps: Vec<(i64, ChunkStatus)> = Vec::new();
        let mut missing = false;
        'outer: for z in -range..=range {
            for x in -range..=range {
                let distance = x.abs().max(z.abs());
                let node = ChunkPos::pack_xz(center.x + x, center.z + z);
                if !self.holders.updating.contains_key(&node) {
                    missing = true;
                    break 'outer;
                }
                deps.push((node, Self::promotion_status(kind, distance)));
            }
        }
        if missing {
            self.finish_promotion(key, kind, false);
            return;
        }
        for (node, dep_status) in &deps {
            self.schedule_chunk_generation_task(*node, *dep_status);
        }
        for (node, dep_status) in deps {
            if let Some(h) = self.holders.holder_mut(node) {
                h.add_waiter(dep_status, Waiter::Promotion(key, kind));
            }
        }
        self.evaluate_promotion(key, kind);
    }

    /// Re-check a pending promotion after one of its dependencies changed.
    pub(crate) fn evaluate_promotion(&mut self, key: i64, kind: PromotionKind) {
        let Some(holder) = self.holders.holder(key) else { return };
        let current = match kind {
            PromotionKind::Full => holder.full_chunk_future,
            PromotionKind::Ticking => holder.ticking_chunk_future,
            PromotionKind::EntityTicking => holder.entity_ticking_chunk_future,
        };
        if !current.is_pending() {
            return;
        }
        let range = Self::promotion_params(kind);
        let center = ChunkPos::unpack(key);
        let mut all_done = true;
        for z in -range..=range {
            for x in -range..=range {
                let distance = x.abs().max(z.abs());
                let node = ChunkPos::pack_xz(center.x + x, center.z + z);
                let dep = Self::promotion_status(kind, distance);
                let Some(h) = self.holders.holder(node) else {
                    self.finish_promotion(key, kind, false);
                    return;
                };
                if h.is_status_disallowed(dep) {
                    self.finish_promotion(key, kind, false);
                    return;
                }
                match h.future_state(dep) {
                    FutureState::Done => {}
                    FutureState::Unloaded => {
                        self.finish_promotion(key, kind, false);
                        return;
                    }
                    FutureState::Pending | FutureState::Absent => all_done = false,
                }
            }
        }
        if !all_done {
            return;
        }
        if kind == PromotionKind::Ticking {
            // `thenApplyAsync(..., mainThreadExecutor)`
            if !self.main_tasks.iter().any(|t| matches!(t, MainTask::TickingReady(k) if *k == key)) {
                self.main_tasks.push_back(MainTask::TickingReady(key));
            }
        } else {
            self.finish_promotion(key, kind, true);
        }
    }

    /// Complete a promotion future (success or failure) and fire what waited on it.
    pub(crate) fn finish_promotion(&mut self, key: i64, kind: PromotionKind, success: bool) {
        let Some(holder) = self.holders.holder_mut(key) else { return };
        let result = if success { PromotionFuture::Done } else { PromotionFuture::Failed };
        match kind {
            PromotionKind::Full => holder.full_chunk_future = result,
            PromotionKind::Ticking => holder.ticking_chunk_future = result,
            PromotionKind::EntityTicking => {
                holder.entity_ticking_chunk_future = result;
                if std::mem::take(&mut holder.release_when_entity_ticking) {
                    self.main_tasks.push_back(MainTask::Distance(DistanceTask::ReleaseSlot { pos: key }));
                }
            }
        }
        let holder = self.holders.holder_mut(key).expect("holder");
        if success {
            let status = match kind {
                PromotionKind::Full => FullChunkStatus::Full,
                PromotionKind::Ticking => FullChunkStatus::BlockTicking,
                PromotionKind::EntityTicking => FullChunkStatus::EntityTicking,
            };
            if holder.pending_full_state_confirmation == Some(status) {
                holder.pending_full_state_confirmation = None;
                self.main_tasks.push_back(MainTask::StatusChange(ChunkPos::unpack(key), status));
            }
        }
        self.check_unload_ready(key);
    }

    // ------------------------------------------------------------------ main loop

    /// Drain worker results into the main task queue (they are `mainThreadExecutor` tasks in vanilla).
    fn drain_worker_results(&mut self) {
        while let Ok(msg) = self.results.try_recv() {
            self.main_tasks.push_back(MainTask::Worker(msg));
        }
    }

    fn run_light_updates(&mut self) {
        if !self.light.has_pending_work() {
            return;
        }
        let holders = &self.holders;
        let queue_level = |key: i64| {
            let max = super::chunk_task_queue::priority_level_count() - 1;
            holders.visible_holder(key).map_or(max, |c| c.get_queue_level().min(max))
        };
        let result = self.light.run_update(&HolderLightChunks(holders), &queue_level);
        for completion in result.completions {
            self.on_light_event(completion);
        }
    }

    /// `ServerChunkCache.MainThreadExecutor.pollTask` minus the distance updates: run one queued task.
    pub fn poll_main_task(&mut self) -> bool {
        self.drain_worker_results();
        self.run_light_updates();
        self.process_ready();
        self.pump_worldgen();
        let Some(task) = self.main_tasks.pop_front() else { return false };
        self.run_main_task(task);
        true
    }

    fn run_main_task(&mut self, task: MainTask) {
        match task {
            MainTask::Worker(msg) => self.handle_worker_message(msg),
            MainTask::Distance(t) => {
                self.distance_manager.run_main_task(t, &mut self.tickets);
                self.queue_distance_tasks();
            }
            MainTask::FullStep(key) => self.events.push(ChunkMapEvent::FullStep(ChunkPos::unpack(key))),
            MainTask::TickingReady(key) => self.events.push(ChunkMapEvent::StartTicking(ChunkPos::unpack(key))),
            MainTask::StatusChange(pos, status) => self.events.push(ChunkMapEvent::FullStatusChanged(pos, status)),
            MainTask::RunTask(id) => self.run_generation_task_slice(id),
        }
        self.process_ready();
    }

    /// Wake everything whose future completed.
    pub(crate) fn process_ready(&mut self) {
        while let Some((_key, waiter)) = self.ready.pop_front() {
            match waiter {
                Waiter::Task(id) => self.queue_task_slice(id),
                Waiter::Promotion(center, kind) => {
                    self.evaluate_promotion(center, kind);
                }
            }
        }
    }

    pub(crate) fn queue_task_slice(&mut self, id: u64) {
        let Some(task) = self.tasks.get(&id) else { return };
        let center = task.center_key();
        let level = self.holders.holder(center).map_or(chunk_level::max_level() + 1, ChunkHolder::get_queue_level);
        self.worldgen_queue.submit(id, center, level);
    }

    /// `ChunkMap.tick(haveTime)`: unloads and eager saves.
    pub fn tick(&mut self, have_time: &dyn Fn() -> bool) {
        self.poi_manager.tick(have_time);
        if !self.settings.no_save {
            self.process_unloads(have_time);
        }
    }

    pub fn tickets_mut(&mut self) -> &mut TicketStorage {
        &mut self.tickets
    }

    /// `DistanceManager.addTicket` helper used by `ServerChunkCache`.
    pub fn add_ticket(&mut self, ticket: Ticket, pos: ChunkPos) {
        self.tickets.add_ticket(ticket, pos);
    }
}

/// `getChunkForLighting(x, z)`: the chunk at the status before `INITIALIZE_LIGHT` (features done), if present.
struct HolderLightChunks<'a>(&'a Holders);

impl mc_light::LightChunkGetter for HolderLightChunks<'_> {
    fn get_chunk_for_lighting(&self, x: i32, z: i32) -> Option<&dyn mc_light::LightChunk> {
        let holder = self.0.holder(ChunkPos::pack_xz(x, z))?;
        if !holder.has_chunk_unchecked(ChunkStatus::InitializeLight.parent()) {
            return None;
        }
        match &holder.slot {
            ChunkSlot::Proto(p) => Some(&**p),
            ChunkSlot::Level(l) => Some(&l.data),
            _ => None,
        }
    }
}
