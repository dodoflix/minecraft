//! `ServerChunkCache`: the level's chunk source (`ChunkSource`) over the [`ChunkMap`], plus the per-tick chunk
//! processing order. Operations that need the live level (blocking `getChunk`, building/ticking/unloading
//! `LevelChunk`s) are driven by `ServerLevel`, which applies the [`ChunkMapEvent`]s this cache produces.

use super::chunk_holder::{ChunkHolder, ChunkSlot, FutureState, PromotionFuture};
use super::chunk_level;
use super::chunk_map::{ChunkMap, ChunkMapEvent, ChunkMapParams};
use super::fastutil::LongOpenHashSet;
use super::{Ticket, TicketType};
use crate::world::level::chunk::LevelChunk;
use crate::world::level::ticket_storage::TicketStorage;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_world::level_storage::SavedDataStorage;
use mc_world::status::ChunkStatus;
use std::collections::HashSet;
use std::path::PathBuf;

/// `TicketStorage.TYPE` id.
fn ticket_storage_id() -> mc_core::ident::Identifier {
    mc_core::ident::Identifier::parse("minecraft:chunk_tickets").expect("valid id")
}

/// What `getChunkFutureMainThread` produced for the caller to wait on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChunkFuture {
    /// `UNLOADED_CHUNK_FUTURE`.
    Unloaded,
    /// The chunk at `status` of holder `key` (done or pending).
    Chunk(i64, ChunkStatus),
}

pub struct ServerChunkCache {
    pub chunk_map: ChunkMap,
    data_storage: SavedDataStorage,
    spawn_enemies: bool,
    /// `chunkHoldersToBroadcast`.
    chunk_holders_to_broadcast: indexmap::IndexSet<i64>,
}

impl ServerChunkCache {
    /// `new ServerChunkCache(...)`: loads the dimension's `chunk_tickets` saved data and creates the chunk map.
    pub fn new(data_folder: PathBuf, mut params: ChunkMapParams, simulation_distance: i32) -> Self {
        if let Err(e) = std::fs::create_dir_all(&data_folder) {
            log::error!("Failed to create dimension data storage directory: {e}");
        }
        let data_storage = SavedDataStorage::new(data_folder);
        let id = ticket_storage_id();
        params.tickets = match data_storage.get(&id) {
            Some(tag) => match TicketStorage::from_tag(&tag) {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Failed to parse saved data for 'SavedDataType[{id}]': {e}");
                    TicketStorage::new()
                }
            },
            None => TicketStorage::new(),
        };
        let mut chunk_map = ChunkMap::new(params);
        chunk_map.distance_manager.update_simulation_distance(simulation_distance, &mut chunk_map.tickets);
        Self { chunk_map, data_storage, spawn_enemies: true, chunk_holders_to_broadcast: indexmap::IndexSet::new() }
    }

    pub fn data_storage(&self) -> &SavedDataStorage {
        &self.data_storage
    }

    // ------------------------------------------------------------------ access

    /// `getChunkNow(x, z)`: the `FULL` chunk if it is loaded.
    pub fn get_chunk_now(&self, x: i32, z: i32) -> Option<&LevelChunk> {
        self.chunk_map.get_visible_chunk_if_present(ChunkPos::pack_xz(x, z))?.get_chunk_if_present_full()
    }

    pub fn get_chunk_now_mut(&mut self, x: i32, z: i32) -> Option<&mut LevelChunk> {
        self.chunk_map.holders.visible_holder_mut(ChunkPos::pack_xz(x, z))?.get_chunk_if_present_full_mut()
    }

    fn chunk_absent(holder: Option<&ChunkHolder>, target_ticket_level: i32) -> bool {
        holder.is_none_or(|h| h.get_ticket_level() > target_ticket_level)
    }

    /// `hasChunk(x, z)`.
    pub fn has_chunk(&self, x: i32, z: i32) -> bool {
        let holder = self.chunk_map.get_visible_chunk_if_present(ChunkPos::pack_xz(x, z));
        !Self::chunk_absent(holder, chunk_level::by_status(ChunkStatus::Full))
    }

    /// `getChunkFutureMainThread(x, z, status, loadOrGenerate)`.
    pub fn get_chunk_future_main_thread(&mut self, x: i32, z: i32, target_status: ChunkStatus, load_or_generate: bool) -> ChunkFuture {
        let pos = ChunkPos::new(x, z);
        let key = pos.pack();
        let target_ticket_level = chunk_level::by_status(target_status);
        if load_or_generate {
            self.add_ticket(Ticket::new(TicketType::UNKNOWN, target_ticket_level), pos);
            if Self::chunk_absent(self.chunk_map.get_visible_chunk_if_present(key), target_ticket_level) {
                self.run_distance_manager_updates();
                if Self::chunk_absent(self.chunk_map.get_visible_chunk_if_present(key), target_ticket_level) {
                    panic!("No chunk holder after ticket has been added");
                }
            }
        }
        if Self::chunk_absent(self.chunk_map.get_visible_chunk_if_present(key), target_ticket_level) {
            return ChunkFuture::Unloaded;
        }
        match self.chunk_map.schedule_chunk_generation_task(key, target_status) {
            FutureState::Unloaded => ChunkFuture::Unloaded,
            _ => ChunkFuture::Chunk(key, target_status),
        }
    }

    /// State of a [`ChunkFuture`]: `Some(true)` done with a chunk, `Some(false)` unloaded, `None` pending.
    pub fn future_result(&self, future: ChunkFuture) -> Option<bool> {
        match future {
            ChunkFuture::Unloaded => Some(false),
            ChunkFuture::Chunk(key, status) => match self.chunk_map.holders.holder(key) {
                None => Some(false),
                Some(h) if h.future_state(status) == FutureState::Done => Some(true),
                Some(h) if h.is_status_disallowed(status) => Some(false),
                Some(_) => None,
            },
        }
    }

    /// The chunk object of a completed future as a proto chunk view (`status` below `FULL`).
    pub fn proto_chunk(&self, key: i64) -> Option<&mc_world::chunk::ProtoChunk> {
        match &self.chunk_map.holders.holder(key)?.slot {
            ChunkSlot::Proto(p) => Some(p),
            _ => None,
        }
    }

    // ------------------------------------------------------------------ updates

    /// `runDistanceManagerUpdates()`.
    pub fn run_distance_manager_updates(&mut self) -> bool {
        let updated = self.chunk_map.run_all_updates();
        let promoted = self.chunk_map.promote_chunk_map();
        self.chunk_map.run_generation_tasks();
        updated || promoted
    }

    /// `pollTask()`: one unit of chunk work. Events for the level are queued in `chunk_map.events`.
    pub fn poll_task(&mut self) -> bool {
        if self.run_distance_manager_updates() {
            return true;
        }
        self.chunk_map.poll_main_task()
    }

    /// Events produced since the last call, for `ServerLevel` to apply.
    pub fn take_events(&mut self) -> Vec<ChunkMapEvent> {
        std::mem::take(&mut self.chunk_map.events)
    }

    /// `ServerChunkCache.tick(haveTime, tickChunks)` minus `tickChunks()` (which needs the level): purges expired
    /// tickets, runs the distance updates and the chunk map tick. Returns whether chunks should be ticked by the level.
    pub fn tick_pre(&mut self, runs_normally: bool, tick_chunks: bool) {
        if runs_normally || !tick_chunks {
            let holders = &self.chunk_map.holders;
            self.chunk_map.tickets.purge_stale_tickets(&|pos| holders.updating.get(&pos).is_none_or(ChunkHolder::is_ready_for_saving));
        }
        self.run_distance_manager_updates();
    }

    /// `ServerChunkCache.tick`'s `chunkMap.tick(haveTime)` part (unloads).
    pub fn tick_unload(&mut self, have_time: &dyn Fn() -> bool) {
        self.chunk_map.tick(have_time);
    }

    /// `broadcastChangedChunks()`.
    pub fn broadcast_changed_chunks(&mut self) {
        for key in std::mem::take(&mut self.chunk_holders_to_broadcast) {
            self.chunk_map.broadcast_changes(key);
        }
    }

    /// `blockChanged(pos)`.
    pub fn block_changed(&mut self, pos: BlockPos) {
        let key = ChunkPos::pack_xz(SectionPos::block_to_section_coord_i32(pos.x()), SectionPos::block_to_section_coord_i32(pos.z()));
        if let Some(holder) = self.chunk_map.holders.visible_holder_mut(key) {
            if holder.block_changed(pos) {
                self.chunk_holders_to_broadcast.insert(key);
            }
        }
    }

    /// `onLightUpdate(layer, pos)`.
    pub fn on_light_update(&mut self, sky: bool, pos: SectionPos) {
        let key = pos.chunk().pack();
        let min = self.chunk_map.level_height.min_section_y() - 1;
        let max = self.chunk_map.level_height.max_section_y() + 1;
        if let Some(holder) = self.chunk_map.holders.visible_holder_mut(key) {
            if holder.section_light_changed(sky, pos.y(), min, max) {
                self.chunk_holders_to_broadcast.insert(key);
            }
        }
    }

    /// Called when a holder's ticking chunk became ready to send: queue pending broadcasts.
    pub fn after_chunk_ready_to_send(&mut self) {
        for key in std::mem::take(&mut self.chunk_map.chunk_ready_to_send) {
            if self.chunk_map.holders.visible_holder(key).is_some_and(ChunkHolder::has_changes_to_broadcast) {
                self.chunk_holders_to_broadcast.insert(key);
            }
        }
    }

    /// `isPositionTicking(chunkKey)` given `level.shouldTickBlocksAt(chunkKey)`.
    pub fn is_position_ticking(&self, chunk_key: i64, should_tick_blocks_at: bool) -> bool {
        if !should_tick_blocks_at {
            return false;
        }
        self.chunk_map.get_visible_chunk_if_present(chunk_key).is_some_and(|h| h.ticking_chunk_future == PromotionFuture::Done)
    }

    // ------------------------------------------------------------------ tickets

    pub fn has_active_tickets(&self) -> bool {
        self.chunk_map.tickets.should_keep_dimension_active()
    }

    pub fn add_ticket(&mut self, ticket: Ticket, pos: ChunkPos) {
        self.chunk_map.tickets.add_ticket(ticket, pos);
    }

    pub fn add_ticket_with_radius(&mut self, ty: TicketType, pos: ChunkPos, radius: i32) {
        self.chunk_map.tickets.add_ticket_with_radius(ty, pos, radius);
    }

    pub fn remove_ticket_with_radius(&mut self, ty: TicketType, pos: ChunkPos, radius: i32) {
        self.chunk_map.tickets.remove_ticket_with_radius(ty, pos, radius);
    }

    /// `updateChunkForced(pos, forced)`.
    pub fn update_chunk_forced(&mut self, pos: ChunkPos, forced: bool) -> bool {
        self.chunk_map.tickets.update_chunk_forced(pos, forced)
    }

    /// `getForceLoadedChunks()`.
    pub fn get_force_loaded_chunks(&self) -> &LongOpenHashSet {
        self.chunk_map.tickets.get_force_loaded_chunks()
    }

    /// `TicketStorage.activateAllDeactivatedTickets()` (server start: persisted tickets become active).
    pub fn activate_all_deactivated_tickets(&mut self) {
        self.chunk_map.tickets.activate_all_deactivated_tickets();
    }

    /// `deactivateTicketsOnClosing()`.
    pub fn deactivate_tickets_on_closing(&mut self) {
        self.chunk_map.tickets.deactivate_tickets_on_closing();
    }

    pub fn set_view_distance(&mut self, new_distance: i32) {
        self.chunk_map.set_server_view_distance(new_distance);
    }

    pub fn set_simulation_distance(&mut self, simulation_distance: i32) {
        self.chunk_map.distance_manager.update_simulation_distance(simulation_distance, &mut self.chunk_map.tickets);
    }

    pub fn set_spawn_settings(&mut self, spawn_enemies: bool) {
        self.spawn_enemies = spawn_enemies;
    }

    pub fn spawn_enemies(&self) -> bool {
        self.spawn_enemies
    }

    /// `getLoadedChunksCount()`.
    pub fn get_loaded_chunks_count(&self) -> usize {
        self.chunk_map.size()
    }

    /// `getChunkDebugData(pos)`.
    pub fn get_chunk_debug_data(&self, pos: ChunkPos) -> String {
        let Some(h) = self.chunk_map.get_visible_chunk_if_present(pos.pack()) else { return "null".to_string() };
        let mut result = format!("{}\n", h.get_ticket_level());
        if let Some(status) = h.get_latest_status() {
            result += &format!("St: \u{a7}{}{}\u{a7}r\n", status.index(), status.name());
        }
        if let Some(status) = h.latest_chunk_status() {
            let persisted = h.get_persisted_status().unwrap_or(status);
            result += &format!("Ch: \u{a7}{}{}\u{a7}r\n", persisted.index(), persisted.name());
        }
        let full = h.get_full_status();
        result += &format!("\u{a7}{}{:?}\u{a7}r", full as i32, full);
        result
    }

    // ------------------------------------------------------------------ persistence

    /// Persist the dirty `chunk_tickets` saved data (`SavedDataStorage` write of the dimension).
    pub fn save_data_storage(&mut self) {
        let tickets = &mut self.chunk_map.tickets;
        if tickets.is_dirty() {
            let id = ticket_storage_id();
            match self.data_storage.save(&id, tickets.to_tag()) {
                Ok(()) => tickets.set_dirty_to(false),
                Err(e) => log::error!("Could not save data {id}: {e}"),
            }
        }
    }

    /// The set of chunk keys this cache has holders for (visible), for diagnostics and tests.
    pub fn visible_keys(&self) -> HashSet<i64> {
        self.chunk_map.holders.visible.iter().copied().collect()
    }
}
