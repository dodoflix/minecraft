//! `ChunkMap`'s player side: which chunks a player tracks (`updateChunkTracking`, `applyChunkTrackingView`), player
//! tickets (`updatePlayerStatus`, `move`), view distance, spawning queries and sending changed blocks.

use super::chunk_holder::ChunkSlot;
use super::chunk_map::{
    ChunkMap, MAX_VIEW_DISTANCE, MIN_VIEW_DISTANCE, PlayerTrackingInfo, PlayerTrackingUpdate,
    TrackedPlayer,
};
use super::chunk_tracking_view::ChunkTrackingView;
use super::distance_manager::TriState;
use crate::world::entity::EntityId;
use crate::world::level::block::state::BlockStateExt;
use crate::world::level::chunk::LevelChunk;
use mc_core::math::mth;
use mc_core::phys::Vec3;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_protocol::packets::play::level::{
    ClientboundBlockUpdatePacket, ClientboundLightUpdatePacket, ClientboundLightUpdatePacketData,
    ClientboundSectionBlocksUpdatePacket, ClientboundSetChunkCacheCenterPacket,
};

/// `SectionPos.of(entity)` for a player position.
fn section_of(position: Vec3) -> SectionPos {
    SectionPos::of_position(&position)
}

impl ChunkMap {
    /// `setServerViewDistance(newViewDistance)`.
    pub fn set_server_view_distance(&mut self, new_view_distance: i32) {
        let actual = mth::clamp_i32(new_view_distance, MIN_VIEW_DISTANCE, MAX_VIEW_DISTANCE);
        if actual != self.server_view_distance {
            self.server_view_distance = actual;
            self.distance_manager.update_player_tickets(actual);
            let ids: Vec<EntityId> = self.player_map.get_all_players().collect();
            for id in ids {
                self.update_chunk_tracking(id);
            }
        }
    }

    pub fn server_view_distance(&self) -> i32 {
        self.server_view_distance
    }

    /// `getPlayerViewDistance(player)`.
    fn get_player_view_distance(&self, id: EntityId) -> i32 {
        let requested = self
            .players
            .get(&id)
            .map_or(MIN_VIEW_DISTANCE, |p| p.info.requested_view_distance);
        mth::clamp_i32(requested, 2, self.server_view_distance)
    }

    /// `isChunkTracked(player, x, z)`.
    pub fn is_chunk_tracked(&self, id: EntityId, chunk_x: i32, chunk_z: i32) -> bool {
        self.players.get(&id).is_some_and(|p| {
            p.view.contains_xz(chunk_x, chunk_z)
                && !p.info.sink.is_pending(ChunkPos::pack_xz(chunk_x, chunk_z))
        })
    }

    fn is_chunk_on_tracked_border(&self, id: EntityId, chunk_x: i32, chunk_z: i32) -> bool {
        if !self.is_chunk_tracked(id, chunk_x, chunk_z) {
            return false;
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                if (dx != 0 || dz != 0) && !self.is_chunk_tracked(id, chunk_x + dx, chunk_z + dz) {
                    return true;
                }
            }
        }
        false
    }

    /// The tracking view of a player (`player.getChunkTrackingView()`).
    pub fn player_chunk_tracking_view(&self, id: EntityId) -> ChunkTrackingView {
        self.players
            .get(&id)
            .map_or(ChunkTrackingView::Empty, |p| p.view)
    }

    fn skip_player(&self, spectator: bool) -> bool {
        spectator && !self.settings.spectators_generate_chunks
    }

    /// `addEntity(player)` -> `updatePlayerStatus(player, true)`.
    pub fn add_player(&mut self, info: PlayerTrackingInfo) {
        let id = info.entity_id;
        let section = section_of(info.position);
        let ignored = self.skip_player(info.spectator);
        self.players.insert(
            id,
            TrackedPlayer {
                info,
                view: ChunkTrackingView::Empty,
                last_section_pos: section,
            },
        );
        self.player_map.add_player(id, ignored);
        if !ignored {
            self.distance_manager
                .add_player(section, id, &mut self.tickets);
        }
        self.update_chunk_tracking(id);
    }

    /// `removeEntity(player)` -> `updatePlayerStatus(player, false)`.
    pub fn remove_player(&mut self, id: EntityId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let last_pos = player.last_section_pos;
        let was_ignored = self.player_map.ignored_or_unknown(id);
        self.player_map.remove_player(id);
        if !was_ignored {
            self.distance_manager
                .remove_player(last_pos, id, &mut self.tickets);
        }
        self.apply_chunk_tracking_view(id, ChunkTrackingView::Empty);
        self.players.shift_remove(&id);
    }

    /// `move(player)`.
    pub fn move_player(&mut self, update: PlayerTrackingUpdate) {
        let id = update.entity_id;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.info.position = update.position;
        player.info.requested_view_distance = update.requested_view_distance;
        player.info.spectator = update.spectator;
        player.info.alive = update.alive;
        let old_section = player.last_section_pos;
        let new_section = section_of(update.position);
        let was_ignored = self.player_map.ignored(id);
        let ignored = self.skip_player(update.spectator);
        let position_changed = old_section != new_section;
        if position_changed || was_ignored != ignored {
            self.players.get_mut(&id).expect("player").last_section_pos = new_section;
            if !was_ignored {
                self.distance_manager
                    .remove_player(old_section, id, &mut self.tickets);
            }
            if !ignored {
                self.distance_manager
                    .add_player(new_section, id, &mut self.tickets);
            }
            if !was_ignored && ignored {
                self.player_map.ignore_player(id);
            }
            if was_ignored && !ignored {
                self.player_map.un_ignore_player(id);
            }
            self.update_chunk_tracking(id);
        }
    }

    /// The live `player.position()` that `playerIsCloseEnoughForSpawning` reads; `move_player` only runs when a move packet
    /// or a camera change reaches the chunk source, a snap or teleport in between must still count.
    pub fn set_player_position(&mut self, id: EntityId, position: Vec3) {
        if let Some(player) = self.players.get_mut(&id) {
            player.info.position = position;
        }
    }

    /// `updateChunkTracking(player)`.
    pub fn update_chunk_tracking(&mut self, id: EntityId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let chunk_pos = section_of(player.info.position).chunk();
        let view_distance = self.get_player_view_distance(id);
        if let ChunkTrackingView::Positioned {
            center,
            view_distance: vd,
        } = player.view
            && center == chunk_pos
            && vd == view_distance
        {
            return;
        }
        self.apply_chunk_tracking_view(id, ChunkTrackingView::of(chunk_pos, view_distance));
    }

    /// `applyChunkTrackingView(player, next)`.
    fn apply_chunk_tracking_view(&mut self, id: EntityId, next: ChunkTrackingView) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let previous = player.view;
        if let ChunkTrackingView::Positioned {
            center: to_center, ..
        } = next
        {
            let from_same_center = matches!(previous, ChunkTrackingView::Positioned { center, .. } if center == to_center);
            if !from_same_center {
                player
                    .info
                    .connection
                    .send(&ClientboundSetChunkCacheCenterPacket {
                        x: to_center.x,
                        z: to_center.z,
                    });
            }
        }
        let mut entered = Vec::new();
        let mut left = Vec::new();
        ChunkTrackingView::difference(&previous, &next, |p| entered.push(p), |p| left.push(p));
        for pos in entered {
            self.mark_chunk_pending_to_send_pos(id, pos);
        }
        let player = self.players.get_mut(&id).expect("player");
        let alive = player.info.alive;
        for pos in left {
            player.info.sink.drop_chunk(pos, alive);
        }
        player.view = next;
    }

    fn mark_chunk_pending_to_send_pos(&mut self, id: EntityId, pos: ChunkPos) {
        if self.get_chunk_to_send(pos.pack()).is_some()
            && let Some(player) = self.players.get_mut(&id)
        {
            player.info.sink.mark_chunk_pending_to_send(pos);
        }
    }

    /// `getChunkToSend(key)`.
    pub fn get_chunk_to_send(&self, key: i64) -> Option<&LevelChunk> {
        let holder = self.get_visible_chunk_if_present(key)?;
        if holder
            .send_sync_light_barriers
            .iter()
            .any(|id| self.light_barrier_pending.contains(id))
        {
            return None;
        }
        holder.get_ticking_chunk()
    }

    /// `onChunkReadyToSend(chunkHolder, chunk)`: tell every player that tracks the chunk.
    pub(crate) fn on_chunk_ready_to_send(&mut self, key: i64) {
        let pos = ChunkPos::unpack(key);
        let ids: Vec<EntityId> = self.player_map.get_all_players().collect();
        for id in ids {
            if self.players.get(&id).is_some_and(|p| p.view.contains(pos)) {
                self.mark_chunk_pending_to_send_pos(id, pos);
            }
        }
        self.chunk_ready_to_send.push(key);
    }

    /// `waitForLightBeforeSending(centerChunk, chunkRadius)`.
    pub fn wait_for_light_before_sending(&mut self, center: ChunkPos, chunk_radius: i32) {
        let affected = chunk_radius + 1;
        for pos in ChunkPos::range_closed(&center, affected) {
            if self.holders.visible_holder(pos.pack()).is_some() {
                let barrier = self.light.wait_for_pending_tasks(pos);
                self.light_barrier_pending.insert(barrier);
                let pending = &self.light_barrier_pending;
                let holder = self
                    .holders
                    .visible_holder_mut(pos.pack())
                    .expect("visible holder");
                // `addSendDependency`: the chunk waits for every barrier added to it.
                holder
                    .send_sync_light_barriers
                    .retain(|id| pending.contains(id));
                holder.send_sync_light_barriers.push(barrier);
            }
        }
    }

    /// `forEachReadyToSendChunk(consumer)`.
    pub fn ready_to_send_chunks(&self) -> Vec<i64> {
        self.holders
            .visible
            .iter()
            .copied()
            .filter(|k| self.get_chunk_to_send(*k).is_some())
            .collect()
    }

    // ------------------------------------------------------------------ spawning queries

    /// `collectSpawningChunks(output)`: keys of ticking chunks with a player close enough to spawn.
    pub fn collect_spawning_chunks(&mut self) -> Vec<i64> {
        let candidates = self.distance_manager.get_spawn_candidate_chunks();
        let mut out = Vec::new();
        for key in candidates {
            if let Some(holder) = self.holders.visible_holder(key)
                && holder.get_ticking_chunk().is_some()
                && self.any_player_close_enough_for_spawning_internal(holder.get_pos())
            {
                out.push(key);
            }
        }
        out
    }

    /// `forEachBlockTickingChunk(consumer)`: keys in the simulation map's iteration order.
    pub fn block_ticking_chunks(&self) -> Vec<i64> {
        let mut out = Vec::new();
        self.distance_manager.for_each_entity_ticking_chunk(|key| {
            if self
                .get_visible_chunk_if_present(key)
                .is_some_and(|h| h.get_ticking_chunk().is_some())
            {
                out.push(key);
            }
        });
        out
    }

    /// `anyPlayerCloseEnoughForSpawning(pos)`.
    pub fn any_player_close_enough_for_spawning(&mut self, pos: ChunkPos) -> bool {
        match self.distance_manager.has_players_nearby(pos.pack()) {
            TriState::Default => self.any_player_close_enough_for_spawning_internal(pos),
            TriState::True => true,
            TriState::False => false,
        }
    }

    /// `anyPlayerCloseEnoughTo(pos, maxDistance)`.
    pub fn any_player_close_enough_to(&self, pos: BlockPos, max_distance: i32) -> bool {
        let target = Vec3::new(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z()));
        self.player_map.get_all_players().any(|id| {
            self.players.get(&id).is_some_and(|p| {
                !p.info.spectator && {
                    let d = p.info.position.distance_to(target);
                    d < f64::from(max_distance)
                }
            })
        })
    }

    fn any_player_close_enough_for_spawning_internal(&self, pos: ChunkPos) -> bool {
        self.player_map
            .get_all_players()
            .any(|id| self.player_is_close_enough_for_spawning(id, pos))
    }

    /// `getPlayersCloseForSpawning(pos)`.
    pub fn get_players_close_for_spawning(&mut self, pos: ChunkPos) -> Vec<EntityId> {
        if self.distance_manager.has_players_nearby(pos.pack()) == TriState::False {
            return Vec::new();
        }
        self.player_map
            .get_all_players()
            .filter(|id| self.player_is_close_enough_for_spawning(*id, pos))
            .collect()
    }

    fn player_is_close_enough_for_spawning(&self, id: EntityId, pos: ChunkPos) -> bool {
        let Some(p) = self.players.get(&id) else {
            return false;
        };
        if p.info.spectator {
            return false;
        }
        let x_pos = f64::from(SectionPos::section_to_block_coord_offset(pos.x, 8));
        let z_pos = f64::from(SectionPos::section_to_block_coord_offset(pos.z, 8));
        let xd = x_pos - p.info.position.x;
        let zd = z_pos - p.info.position.z;
        xd * xd + zd * zd < 16384.0
    }

    /// `getPlayers(pos, borderOnly)`.
    pub fn get_players(&self, pos: ChunkPos, border_only: bool) -> Vec<EntityId> {
        self.player_map
            .get_all_players()
            .filter(|id| {
                if border_only {
                    self.is_chunk_on_tracked_border(*id, pos.x, pos.z)
                } else {
                    self.is_chunk_tracked(*id, pos.x, pos.z)
                }
            })
            .collect()
    }

    // ------------------------------------------------------------------ block change broadcasting

    /// `ChunkHolder.broadcastChanges(chunk)` for the holder at `key`.
    pub fn broadcast_changes(&mut self, key: i64) {
        let Some(holder) = self.holders.visible_holder(key) else {
            return;
        };
        if !holder.has_changes_to_broadcast() || holder.get_ticking_chunk().is_none() {
            return;
        }
        let pos = holder.get_pos();
        let (sky, block) = {
            let holder = self.holders.visible_holder_mut(key).expect("holder");
            if !holder.sky_changed_light_section_filter.is_empty()
                || !holder.block_changed_light_section_filter.is_empty()
            {
                holder.take_light_filters()
            } else {
                Default::default()
            }
        };
        if !sky.is_empty() || !block.is_empty() {
            let border_players = self.get_players(pos, true);
            if !border_players.is_empty() {
                {
                    let to_protocol = |b: &mc_world::chunk_packet::BitSet| {
                        mc_protocol::codec::BitSet::from_bytes(&b.to_bytes())
                    };
                    let to_bits = |set: &std::collections::HashSet<i32>| {
                        let mut bits = mc_world::chunk_packet::BitSet::new();
                        for i in set {
                            bits.set(*i as usize);
                        }
                        bits
                    };
                    let data = self.light.engine().light_update_packet_data(
                        pos,
                        Some(&to_bits(&sky)),
                        Some(&to_bits(&block)),
                    );
                    let packet = ClientboundLightUpdatePacket {
                        x: pos.x,
                        z: pos.z,
                        light_data: ClientboundLightUpdatePacketData {
                            sky_y_mask: to_protocol(&data.sky_y_mask),
                            block_y_mask: to_protocol(&data.block_y_mask),
                            empty_sky_y_mask: to_protocol(&data.empty_sky_y_mask),
                            empty_block_y_mask: to_protocol(&data.empty_block_y_mask),
                            sky_updates: data.sky_updates,
                            block_updates: data.block_updates,
                        },
                    };
                    for id in border_players {
                        if let Some(p) = self.players.get(&id) {
                            p.info.connection.send(&packet);
                        }
                    }
                }
            }
        }
        let holder = self.holders.visible_holder_mut(key).expect("holder");
        if holder.has_changed_sections() {
            let changes = holder.take_changed_blocks();
            let players = self.get_players(pos, false);
            let holder = self.holders.visible_holder(key).expect("holder");
            let Some(ChunkSlot::Level(chunk)) = Some(&holder.slot) else {
                return;
            };
            for (section_index, changed) in changes {
                if players.is_empty() {
                    continue;
                }
                let section_y = self
                    .level_height
                    .section_y_from_section_index(section_index as i32);
                let section_pos = SectionPos::of_chunk(&pos, section_y);
                if changed.len() == 1 {
                    let bp = section_pos.relative_to_block_pos(changed[0]);
                    let state = chunk.data.get_block_state(bp);
                    let packet = ClientboundBlockUpdatePacket {
                        pos: bp,
                        block_state: state,
                    };
                    self.send_to(&players, &packet);
                    self.broadcast_block_entity_if_needed(&players, chunk, bp, state);
                } else {
                    let section = chunk.data.access.get_section(section_index);
                    let changes: Vec<(u16, mc_data::block::BlockState)> = changed
                        .iter()
                        .map(|&rel| {
                            let x = SectionPos::section_relative_x(rel);
                            let y = SectionPos::section_relative_y(rel);
                            let z = SectionPos::section_relative_z(rel);
                            (rel as u16, section.get_block_state(x, y, z))
                        })
                        .collect();
                    let packet = ClientboundSectionBlocksUpdatePacket {
                        section_pos,
                        changes: changes.clone(),
                    };
                    self.send_to(&players, &packet);
                    for (rel, state) in changes {
                        let bp = section_pos.relative_to_block_pos(rel as i16);
                        self.broadcast_block_entity_if_needed(&players, chunk, bp, state);
                    }
                }
            }
        }
    }

    fn send_to<P: mc_protocol::packet::Packet>(&self, players: &[EntityId], packet: &P) {
        for id in players {
            if let Some(p) = self.players.get(id) {
                p.info.connection.send(packet);
            }
        }
    }

    fn broadcast_block_entity_if_needed(
        &self,
        players: &[EntityId],
        chunk: &LevelChunk,
        pos: BlockPos,
        state: mc_data::block::BlockState,
    ) {
        if state.has_block_entity()
            && let Some(be) = chunk.block_entities.get(&pos)
            && let Some(packet) = be.get_update_packet()
        {
            self.send_to(players, &packet);
        }
    }
}
