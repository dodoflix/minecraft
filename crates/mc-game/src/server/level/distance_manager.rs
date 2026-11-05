//! `DistanceManager`: turns tickets and player positions into chunk levels (`LoadingChunkTracker`,
//! `SimulationChunkTracker`, the natural spawn counter and the throttled player loading tickets).
//!
//! The chunk-holder side of vanilla's abstract methods (`isChunkToRemove`, `getChunk`, `updateChunkScheduling`) is the
//! [`DistanceHost`] trait implemented by `ChunkMap`'s holder store; `ChunkMap` runs the holder updates after
//! [`DistanceManager::run_updates`].

use super::chunk_level;
use super::chunk_task_queue::ChunkTaskDispatcher;
use super::dynamic_graph::{ChunkTrackerBehavior, GraphState};
use super::fastutil::{LongOpenHashMap, LongOpenHashSet};
use super::{FullChunkStatus, Ticket, TicketType};
use crate::world::entity::EntityId;
use crate::world::level::ticket_storage::{ChunkUpdate, TicketStorage};
use mc_core::pos::{ChunkPos, SectionPos};

/// `TriState` of `hasPlayersNearby`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriState {
    True,
    False,
    Default,
}

/// The chunk-holder lookups the loading tracker needs (`DistanceManager`'s abstract methods).
pub trait DistanceHost {
    /// `isChunkToRemove(node)`.
    fn is_chunk_to_remove(&self, node: i64) -> bool;
    /// `getChunk(node).getTicketLevel()`.
    fn holder_ticket_level(&self, node: i64) -> Option<i32>;
    /// `updateChunkScheduling(node, level, chunk, oldLevel)`: returns the holder (by position key) whose futures need
    /// updating, if any.
    fn update_chunk_scheduling(&mut self, node: i64, level: i32, old_level: i32) -> bool;
}

/// A deferred main-thread action of the ticket throttler.
#[derive(Clone, Debug)]
pub enum DistanceTask {
    /// `ticketDispatcher.submit` task body: add the `PLAYER_LOADING` ticket if the player still sees the chunk.
    AddPlayerLoadingTicket { key: i64, ticket: Ticket },
    /// `release(key, () -> execute(() -> removeTicket(key, ticket)), true)`.
    RemovePlayerLoadingTicket { key: i64, ticket: Ticket },
    /// `ticketDispatcher.release(pos, () -> {}, false)` once the chunk is entity ticking.
    ReleaseSlot { pos: i64 },
}

/// `FixedPlayerDistanceChunkTracker` state.
struct FixedPlayerDistance {
    state: GraphState,
    chunks: LongOpenHashMap<u8>,
    max_distance: i32,
}

impl FixedPlayerDistance {
    fn new(max_distance: i32) -> Self {
        Self { state: GraphState::new(max_distance + 2), chunks: LongOpenHashMap::new(), max_distance }
    }
}

struct FixedBehavior<'a> {
    chunks: &'a mut LongOpenHashMap<u8>,
    max_distance: i32,
    players_per_chunk: &'a LongOpenHashMap<Vec<EntityId>>,
    /// `onLevelChange(node, oldLevel, level)` sink (the player ticket tracker's `toUpdate`).
    to_update: Option<&'a mut LongOpenHashSet>,
}

impl ChunkTrackerBehavior for FixedBehavior<'_> {
    fn get_level(&mut self, node: i64) -> i32 {
        self.chunks.get(node).map_or(self.max_distance + 2, |&l| i32::from(l))
    }

    fn set_level(&mut self, node: i64, level: i32) {
        if level > self.max_distance {
            self.chunks.remove(node);
        } else {
            self.chunks.insert(node, level as u8);
        }
        if let Some(to_update) = self.to_update.as_deref_mut() {
            to_update.add(node);
        }
    }

    fn get_level_from_source(&mut self, to: i64) -> i32 {
        if self.players_per_chunk.get(to).is_some_and(|p| !p.is_empty()) { 0 } else { i32::MAX }
    }
}

struct LoadingBehavior<'a, H: DistanceHost> {
    host: &'a mut H,
    tickets: &'a TicketStorage,
    /// `chunksToUpdateFutures` (insertion order, deduplicated).
    chunks_to_update_futures: &'a mut super::fastutil::ConstHashReferenceSet,
}

impl<H: DistanceHost> ChunkTrackerBehavior for LoadingBehavior<'_, H> {
    fn get_level(&mut self, node: i64) -> i32 {
        if !self.host.is_chunk_to_remove(node) {
            if let Some(level) = self.host.holder_ticket_level(node) {
                return level;
            }
        }
        chunk_level::max_level() + 1
    }

    fn set_level(&mut self, node: i64, level: i32) {
        let max = chunk_level::max_level() + 1;
        let old_level = self.host.holder_ticket_level(node).unwrap_or(max);
        if old_level != level && self.host.update_chunk_scheduling(node, level, old_level) {
            self.chunks_to_update_futures.insert(node);
        }
    }

    fn get_level_from_source(&mut self, to: i64) -> i32 {
        self.tickets.get_ticket_level_at(to, false)
    }
}

struct SimulationBehavior<'a> {
    chunks: &'a mut LongOpenHashMap<u8>,
    tickets: &'a TicketStorage,
}

impl ChunkTrackerBehavior for SimulationBehavior<'_> {
    fn get_level(&mut self, node: i64) -> i32 {
        self.chunks.get(node).map_or(33, |&l| i32::from(l))
    }

    fn set_level(&mut self, node: i64, level: i32) {
        if level >= 33 {
            self.chunks.remove(node);
        } else {
            self.chunks.insert(node, level as u8);
        }
    }

    fn get_level_from_source(&mut self, to: i64) -> i32 {
        self.tickets.get_ticket_level_at(to, true)
    }
}

pub struct DistanceManager {
    players_per_chunk: LongOpenHashMap<Vec<EntityId>>,
    loading_state: GraphState,
    simulation_state: GraphState,
    simulation_chunks: LongOpenHashMap<u8>,
    natural_spawn_chunk_counter: FixedPlayerDistance,
    player_ticket_manager: FixedPlayerDistance,
    player_ticket_to_update: LongOpenHashSet,
    player_ticket_view_distance: i32,
    /// `chunksToUpdateFutures`: holders (by chunk key) whose ticket level changed.
    pub chunks_to_update_futures: super::fastutil::ConstHashReferenceSet,
    ticket_dispatcher: ChunkTaskDispatcher<DistanceTask, DistanceTask>,
    /// `ticketsToRelease`.
    pub tickets_to_release: LongOpenHashSet,
    simulation_distance: i32,
    /// Main-thread tasks produced by the throttler, for `ChunkMap` to enqueue.
    pub main_tasks: Vec<DistanceTask>,
}

fn player_ticket_level() -> i32 {
    chunk_level::by_full_status(FullChunkStatus::EntityTicking)
}

impl DistanceManager {
    pub fn new() -> Self {
        Self {
            players_per_chunk: LongOpenHashMap::new(),
            loading_state: GraphState::new(chunk_level::max_level() + 2),
            simulation_state: GraphState::new(34),
            simulation_chunks: LongOpenHashMap::new(),
            natural_spawn_chunk_counter: FixedPlayerDistance::new(8),
            player_ticket_manager: FixedPlayerDistance::new(32),
            player_ticket_to_update: LongOpenHashSet::new(),
            player_ticket_view_distance: 0,
            chunks_to_update_futures: Default::default(),
            ticket_dispatcher: ChunkTaskDispatcher::new(Some(4), 34),
            tickets_to_release: LongOpenHashSet::new(),
            simulation_distance: 10,
            main_tasks: Vec::new(),
        }
    }

    /// Feed queued `TicketStorage` notifications into the trackers (vanilla's synchronous listeners).
    fn drain_ticket_updates<H: DistanceHost>(&mut self, host: &mut H, tickets: &mut TicketStorage) {
        let loading: Vec<ChunkUpdate> = std::mem::take(&mut tickets.loading_updates);
        let simulation: Vec<ChunkUpdate> = std::mem::take(&mut tickets.simulation_updates);
        for u in loading {
            let mut b = LoadingBehavior { host, tickets, chunks_to_update_futures: &mut self.chunks_to_update_futures };
            self.loading_state.chunk_tracker_update(&mut b, u.node, u.new_level_from, u.only_decreased);
        }
        for u in simulation {
            let mut b = SimulationBehavior { chunks: &mut self.simulation_chunks, tickets };
            self.simulation_state.chunk_tracker_update(&mut b, u.node, u.new_level_from, u.only_decreased);
        }
    }

    fn run_natural_spawn_updates(&mut self) {
        let mut b = FixedBehavior {
            chunks: &mut self.natural_spawn_chunk_counter.chunks,
            max_distance: self.natural_spawn_chunk_counter.max_distance,
            players_per_chunk: &self.players_per_chunk,
            to_update: None,
        };
        self.natural_spawn_chunk_counter.state.run_chunk_tracker_updates(&mut b, i32::MAX);
    }

    fn run_player_ticket_updates(&mut self) {
        let mut b = FixedBehavior {
            chunks: &mut self.player_ticket_manager.chunks,
            max_distance: self.player_ticket_manager.max_distance,
            players_per_chunk: &self.players_per_chunk,
            to_update: Some(&mut self.player_ticket_to_update),
        };
        self.player_ticket_manager.state.run_chunk_tracker_updates(&mut b, i32::MAX);
        if !self.player_ticket_to_update.is_empty() {
            let nodes: Vec<i64> = self.player_ticket_to_update.iter().collect();
            for node in nodes {
                let old_level = self.ticket_dispatcher.queue_level(node);
                let level = self.player_ticket_manager.chunks.get(node).map_or(34, |&l| i32::from(l));
                if old_level != level {
                    self.ticket_dispatcher.on_level_change(node, level);
                    self.player_ticket_on_level_change(node, level, self.have_ticket_for(old_level), self.have_ticket_for(level));
                }
            }
            self.player_ticket_to_update.clear();
        }
    }

    fn have_ticket_for(&self, level: i32) -> bool {
        level <= self.player_ticket_view_distance
    }

    /// `PlayerTicketTracker.onLevelChange(key, level, saw, sees)`.
    fn player_ticket_on_level_change(&mut self, key: i64, level: i32, saw: bool, sees: bool) {
        if saw != sees {
            let ticket = Ticket::new(TicketType::PLAYER_LOADING, player_ticket_level());
            if sees {
                self.ticket_dispatcher.submit(DistanceTask::AddPlayerLoadingTicket { key, ticket }, key, level);
            } else {
                self.ticket_dispatcher.release(key, Some(DistanceTask::RemovePlayerLoadingTicket { key, ticket }), true);
            }
        }
    }

    /// Run the throttler's queued messages; task bodies and release actions become main-thread tasks.
    pub fn process_ticket_dispatcher(&mut self) {
        let mut tasks = Vec::new();
        let mut releases = Vec::new();
        self.ticket_dispatcher.process(|_, t| tasks.push(t), |r| releases.push(r));
        self.main_tasks.extend(tasks);
        self.main_tasks.extend(releases);
    }

    /// The tracker part of `runAllUpdates`: returns `updated` (loading tracker did work). The caller then runs
    /// `updateHighestAllowedStatus` / `updateFutures` on [`Self::chunks_to_update_futures`] (or handles
    /// `tickets_to_release` when that is empty).
    pub fn run_updates<H: DistanceHost>(&mut self, host: &mut H, tickets: &mut TicketStorage) -> bool {
        self.drain_ticket_updates(host, tickets);
        self.run_natural_spawn_updates();
        self.run_simulation_updates(tickets);
        self.run_player_ticket_updates();
        self.process_ticket_dispatcher();
        let mut b = LoadingBehavior { host, tickets, chunks_to_update_futures: &mut self.chunks_to_update_futures };
        let left = self.loading_state.run_chunk_tracker_updates(&mut b, i32::MAX);
        i32::MAX - left != 0
    }

    fn run_simulation_updates(&mut self, tickets: &TicketStorage) {
        let mut b = SimulationBehavior { chunks: &mut self.simulation_chunks, tickets };
        self.simulation_state.run_chunk_tracker_updates(&mut b, i32::MAX);
    }

    /// `addPlayer(pos, player)`.
    pub fn add_player(&mut self, pos: SectionPos, player: EntityId, tickets: &mut TicketStorage) {
        let chunk = pos.chunk();
        let chunk_pos = chunk.pack();
        let players = self.players_per_chunk.get_or_insert_with(chunk_pos, Vec::new);
        if !players.contains(&player) {
            players.push(player);
        }
        self.natural_spawn_update(chunk_pos, 0, true);
        self.player_ticket_update(chunk_pos, 0, true);
        tickets.add_ticket(Ticket::new(TicketType::PLAYER_SIMULATION, self.get_player_ticket_level()), chunk);
    }

    /// `removePlayer(pos, player)`.
    pub fn remove_player(&mut self, pos: SectionPos, player: EntityId, tickets: &mut TicketStorage) {
        let chunk = pos.chunk();
        let chunk_pos = chunk.pack();
        let Some(players) = self.players_per_chunk.get_mut(chunk_pos) else { return };
        players.retain(|p| *p != player);
        if players.is_empty() {
            self.players_per_chunk.remove(chunk_pos);
            self.natural_spawn_update(chunk_pos, i32::MAX, false);
            self.player_ticket_update(chunk_pos, i32::MAX, false);
            tickets.remove_ticket(&Ticket::new(TicketType::PLAYER_SIMULATION, self.get_player_ticket_level()), chunk);
        }
    }

    fn natural_spawn_update(&mut self, node: i64, new_level_from: i32, only_decreased: bool) {
        let t = &mut self.natural_spawn_chunk_counter;
        let mut b = FixedBehavior { chunks: &mut t.chunks, max_distance: t.max_distance, players_per_chunk: &self.players_per_chunk, to_update: None };
        t.state.chunk_tracker_update(&mut b, node, new_level_from, only_decreased);
    }

    fn player_ticket_update(&mut self, node: i64, new_level_from: i32, only_decreased: bool) {
        let t = &mut self.player_ticket_manager;
        let mut b = FixedBehavior {
            chunks: &mut t.chunks,
            max_distance: t.max_distance,
            players_per_chunk: &self.players_per_chunk,
            to_update: Some(&mut self.player_ticket_to_update),
        };
        t.state.chunk_tracker_update(&mut b, node, new_level_from, only_decreased);
    }

    fn get_player_ticket_level(&self) -> i32 {
        (chunk_level::by_full_status(FullChunkStatus::EntityTicking) - self.simulation_distance).max(0)
    }

    pub fn in_entity_ticking_range(&self, key: i64) -> bool {
        chunk_level::is_entity_ticking(self.simulation_level(key))
    }

    pub fn in_block_ticking_range(&self, key: i64) -> bool {
        chunk_level::is_block_ticking(self.simulation_level(key))
    }

    fn simulation_level(&self, key: i64) -> i32 {
        self.simulation_chunks.get(key).map_or(33, |&l| i32::from(l))
    }

    /// `getChunkLevel(key, simulation)`; the loading level needs the holder lookups, so it takes the host.
    pub fn get_chunk_level<H: DistanceHost>(&self, host: &mut H, key: i64, simulation: bool) -> i32 {
        if simulation {
            self.simulation_level(key)
        } else {
            if !host.is_chunk_to_remove(key) {
                if let Some(level) = host.holder_ticket_level(key) {
                    return level;
                }
            }
            chunk_level::max_level() + 1
        }
    }

    /// `updatePlayerTickets(viewDistance)`.
    pub fn update_player_tickets(&mut self, view_distance: i32) {
        let entries: Vec<(i64, i32)> = self.player_ticket_manager.chunks.iter().map(|(k, &l)| (k, i32::from(l))).collect();
        for (key, level) in entries {
            self.player_ticket_on_level_change(key, level, self.have_ticket_for(level), level <= view_distance);
        }
        self.player_ticket_view_distance = view_distance;
    }

    /// `updateSimulationDistance(newDistance)`.
    pub fn update_simulation_distance(&mut self, new_distance: i32, tickets: &mut TicketStorage) {
        if new_distance != self.simulation_distance {
            self.simulation_distance = new_distance;
            tickets.replace_ticket_level_of_type(self.get_player_ticket_level(), TicketType::PLAYER_SIMULATION);
        }
    }

    pub fn get_natural_spawn_chunk_count(&mut self) -> usize {
        self.run_natural_spawn_updates();
        self.natural_spawn_chunk_counter.chunks.len()
    }

    /// `hasPlayersNearby(pos)`.
    pub fn has_players_nearby(&mut self, pos: i64) -> TriState {
        self.run_natural_spawn_updates();
        let level = self.natural_spawn_chunk_counter.chunks.get(pos).map_or(10, |&l| i32::from(l));
        if level <= INSCRIBED_SQUARE_SPAWN_DISTANCE_CHUNK {
            TriState::True
        } else if level > 8 {
            TriState::False
        } else {
            TriState::Default
        }
    }

    /// `forEachEntityTickingChunk` (fastutil iteration order of the simulation map).
    pub fn for_each_entity_ticking_chunk(&self, mut consumer: impl FnMut(i64)) {
        for (key, &level) in self.simulation_chunks.iter() {
            if chunk_level::is_entity_ticking(i32::from(level)) {
                consumer(key);
            }
        }
    }

    /// `getSpawnCandidateChunks()`.
    pub fn get_spawn_candidate_chunks(&mut self) -> Vec<i64> {
        self.run_natural_spawn_updates();
        self.natural_spawn_chunk_counter.chunks.keys()
    }

    pub fn get_debug_status(&self) -> String {
        self.ticket_dispatcher.debug_status("player ticket throttler")
    }

    /// `ticketDispatcher.release(pos, () -> {}, false)`.
    pub fn release_slot(&mut self, pos: i64) {
        self.ticket_dispatcher.release(pos, None, false);
        self.process_ticket_dispatcher();
    }

    /// Execute a main-thread [`DistanceTask`].
    pub fn run_main_task(&mut self, task: DistanceTask, tickets: &mut TicketStorage) {
        match task {
            DistanceTask::AddPlayerLoadingTicket { key, ticket } => {
                let level = self.player_ticket_manager.chunks.get(key).map_or(34, |&l| i32::from(l));
                if self.have_ticket_for(level) {
                    tickets.add_ticket_at(key, ticket);
                    self.tickets_to_release.add(key);
                } else {
                    self.release_slot(key);
                }
            }
            DistanceTask::RemovePlayerLoadingTicket { key, ticket } => {
                tickets.remove_ticket_at(key, &ticket);
            }
            DistanceTask::ReleaseSlot { pos } => self.release_slot(pos),
        }
    }

    pub fn simulation_distance(&self) -> i32 {
        self.simulation_distance
    }
}

impl Default for DistanceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// `NaturalSpawner.INSCRIBED_SQUARE_SPAWN_DISTANCE_CHUNK` = `floor(sqrt(8 * 8 / 2)) ...` as vanilla computes it.
pub const INSCRIBED_SQUARE_SPAWN_DISTANCE_CHUNK: i32 = 5;

/// Helper so tests and `ChunkMap` can compute a chunk key.
pub fn chunk_key(x: i32, z: i32) -> i64 {
    ChunkPos::pack_xz(x, z)
}
