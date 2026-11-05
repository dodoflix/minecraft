//! Wiring of the scoreboard, the custom boss events and the world border into `MinecraftServer` / `ServerLevel`:
//! load/store of the saved data, the conversion of the model's packet payloads to the real play packets, the
//! `PlayerList.addWorldborderListener` broadcasts and `PlayerList.updateEntireScoreboard`.

use super::MinecraftServer;
use super::bossevents::CustomBossEvents;
use crate::server::level::ServerLevel;
use crate::server::players::PlayerHandle;
use crate::world::level::DimensionKey;
use crate::world::level::saveddata::world_border::{BorderEvent, WorldBorder};
use crate::world::scores::packets::{ScoreboardOutput, ScoreboardPacket, SetPlayerTeam};
use crate::world::scores::{NumberFormat, ScoreboardSaveData};
use mc_core::pos::BlockPos;
use mc_protocol::packets::play as wire;

fn wire_number_format(f: &NumberFormat) -> wire::NumberFormat {
    match f {
        NumberFormat::Blank => wire::NumberFormat::Blank,
        NumberFormat::Fixed(c) => wire::NumberFormat::Fixed(c.clone()),
        NumberFormat::Styled(s) => wire::NumberFormat::Styled(s.clone()),
    }
}

fn wire_team(team: &SetPlayerTeam) -> wire::ClientboundSetPlayerTeamPacket {
    let parameters = team.parameters.as_ref().map(|p| wire::TeamParameters {
        display_name: p.display_name.clone(),
        player_prefix: p.player_prefix.clone(),
        player_suffix: p.player_suffix.clone(),
        name_tag_visibility: wire::TeamVisibility::by_id(p.name_tag_visibility.id()),
        collision_rule: wire::TeamCollisionRule::by_id(p.collision_rule.id()),
        color: p.color.map(|c| wire::TeamColor::by_id(c.id())),
        options: p.options as i8,
    });
    let players = team.players.clone();
    let method = match (team.method, parameters) {
        (0, Some(parameters)) => wire::TeamMethod::Add { parameters, players },
        (1, _) => wire::TeamMethod::Remove,
        (2, Some(parameters)) => wire::TeamMethod::Change { parameters },
        (3, _) => wire::TeamMethod::Join { players },
        (4, _) => wire::TeamMethod::Leave { players },
        (m, _) => wire::TeamMethod::Other(m as i8),
    };
    wire::ClientboundSetPlayerTeamPacket { name: team.name.clone(), method }
}

/// Sends one scoreboard packet payload through `send` (which gets the concrete packet type via a generic closure
/// object: the callers need either a broadcast or a single connection).
pub trait PacketSink {
    fn send<P: mc_protocol::Packet>(&self, packet: &P);
}

impl PacketSink for crate::server::players::PlayerList {
    fn send<P: mc_protocol::Packet>(&self, packet: &P) {
        self.broadcast_all(packet);
    }
}

impl PacketSink for PlayerHandle {
    fn send<P: mc_protocol::Packet>(&self, packet: &P) {
        self.connection.send(packet);
    }
}

/// Converts and sends a model packet.
pub fn send_scoreboard_packet(sink: &impl PacketSink, packet: &ScoreboardPacket) {
    match packet {
        ScoreboardPacket::SetScore { owner, objective_name, value, display, number_format } => sink.send(&wire::ClientboundSetScorePacket {
            owner: owner.clone(),
            objective_name: objective_name.clone(),
            score: *value,
            display: display.clone(),
            number_format: number_format.as_ref().map(wire_number_format),
        }),
        ScoreboardPacket::ResetScore { owner, objective_name } => {
            sink.send(&wire::ClientboundResetScorePacket { owner: owner.clone(), objective_name: objective_name.clone() })
        }
        ScoreboardPacket::SetDisplayObjective { slot, objective_name } => sink.send(&wire::ClientboundSetDisplayObjectivePacket {
            slot: wire::DisplaySlot::by_id(slot.id()),
            objective_name: objective_name.clone().unwrap_or_default(),
        }),
        ScoreboardPacket::SetObjective(o) => {
            let settings = wire::ObjectiveSettings {
                display_name: o.display_name.clone(),
                render_type: match o.render_type {
                    crate::world::scores::RenderType::Integer => wire::ObjectiveRenderType::Integer,
                    crate::world::scores::RenderType::Hearts => wire::ObjectiveRenderType::Hearts,
                },
                number_format: o.number_format.as_ref().map(wire_number_format),
            };
            let method = match o.method {
                0 => wire::ObjectiveMethod::Add(settings),
                1 => wire::ObjectiveMethod::Remove,
                2 => wire::ObjectiveMethod::Change(settings),
                m => wire::ObjectiveMethod::Other(m as i8),
            };
            sink.send(&wire::ClientboundSetObjectivePacket { objective_name: o.objective_name.clone(), method })
        }
        ScoreboardPacket::SetPlayerTeam(t) => sink.send(&wire_team(t)),
    }
}

impl MinecraftServer {
    /// `getCustomBossEvents()`.
    pub fn custom_boss_events(&mut self) -> &mut CustomBossEvents {
        self.saved_data.compute_if_absent(&self.saved_types.custom_boss_events)
    }

    /// `scoreboard.load(savedDataStorage.computeIfAbsent(ScoreboardSaveData.TYPE).getData())`.
    pub(super) fn load_scoreboard(&mut self) {
        let data = self.saved_data.compute_if_absent(&self.saved_types.scoreboard).get_data().clone();
        self.scoreboard.write().load(&data);
        // `this.stopwatches = computeIfAbsent(Stopwatches.TYPE)`; the overworld's `WanderingTraderSpawner` loads its data
        self.saved_data.compute_if_absent(&self.saved_types.stopwatches);
        self.saved_data.compute_if_absent(&self.saved_types.wandering_trader);
    }

    /// `scoreboard.storeToSaveDataIfDirty(...)` (first step of `saveAllChunks`).
    pub(super) fn store_scoreboard(&mut self) {
        let ty = self.saved_types.scoreboard.clone();
        let data: &mut ScoreboardSaveData = self.saved_data.compute_if_absent(&ty);
        self.scoreboard.write().store_to_save_data_if_dirty(data);
    }

    /// Sends what the scoreboard queued (`ServerScoreboard`'s `broadcastAll`s) to the players.
    pub fn flush_scoreboard(&mut self) {
        let outputs = self.scoreboard.write().take_outputs();
        for output in outputs {
            match output {
                ScoreboardOutput::Broadcast(packet) => send_scoreboard_packet(&self.player_list, &packet),
                // `ServerScoreboard.updatePlayerWaypoint`: the player's level remakes the connections of that player
                ScoreboardOutput::RemakeWaypointConnections(names) => {
                    for name in names {
                        if let Some(player) = crate::commands::host::player_by_name(self, &name)
                            && let Some(level) = self.level_mut(&player.dimension)
                        {
                            level.waypoint_remake_connections(player.id);
                        }
                    }
                }
            }
        }
    }

    /// `PlayerList.updateEntireScoreboard(scoreboard, player)`.
    pub fn update_entire_scoreboard(&self, player: &PlayerHandle) {
        let scoreboard = self.scoreboard.read();
        for team in scoreboard.get_player_teams() {
            send_scoreboard_packet(player, &ScoreboardPacket::SetPlayerTeam(team.create_add_or_modify_packet(true)));
        }
        let mut seen: Vec<&str> = Vec::new();
        for slot in crate::world::scores::DisplaySlot::VALUES {
            if let Some(objective) = scoreboard.get_display_objective(slot)
                && !seen.contains(&objective.get_name())
            {
                for packet in scoreboard.get_start_tracking_packets(objective) {
                    send_scoreboard_packet(player, &packet);
                }
                seen.push(objective.get_name());
            }
        }
    }

    /// `PlayerList.addWorldborderListener`'s broadcasts: every border change since the last call is sent to the
    /// players of that level (`broadcastAll(packet, level.dimension())`).
    pub fn flush_world_border_events(&mut self) {
        for dimension in self.level_keys() {
            let Some(level) = self.level_mut(&dimension) else { continue };
            let Some(border) = level.world_border_mut() else { continue };
            let events = border.take_events();
            if events.is_empty() {
                continue;
            }
            let border = border.clone();
            for event in events {
                self.broadcast_border_event(&dimension, &border, event);
            }
        }
    }

    fn broadcast_border_event(&self, dimension: &DimensionKey, border: &WorldBorder, event: BorderEvent) {
        let list = &self.player_list;
        match event {
            BorderEvent::SetSize(_) => list.broadcast_all_in_dimension(&wire::ClientboundSetBorderSizePacket { size: border.get_lerp_target() }, dimension),
            BorderEvent::LerpSize { .. } => list.broadcast_all_in_dimension(
                &wire::ClientboundSetBorderLerpSizePacket { old_size: border.get_size(), new_size: border.get_lerp_target(), lerp_time: border.get_lerp_time() },
                dimension,
            ),
            BorderEvent::SetCenter { .. } => list.broadcast_all_in_dimension(
                &wire::ClientboundSetBorderCenterPacket { new_center_x: border.get_center_x(), new_center_z: border.get_center_z() },
                dimension,
            ),
            BorderEvent::SetWarningTime(_) => {
                list.broadcast_all_in_dimension(&wire::ClientboundSetBorderWarningDelayPacket { warning_delay: border.get_warning_time() }, dimension)
            }
            BorderEvent::SetWarningBlocks(_) => {
                list.broadcast_all_in_dimension(&wire::ClientboundSetBorderWarningDistancePacket { warning_blocks: border.get_warning_blocks() }, dimension)
            }
            // `onSetDamagePerBlock` / `onSetSafeZone` have no packet
            BorderEvent::SetDamagePerBlock(_) | BorderEvent::SetSafeZone(_) => {}
        }
    }

    /// `MinecraftServer.createLevels` tail: `level.getWorldBorder().setAbsoluteMaxSize(getAbsoluteMaxWorldSize())` for
    /// every level; the border listeners are installed afterwards, so what the initial settings queued is dropped.
    pub(super) fn init_world_borders(&mut self) {
        let max = self.get_absolute_max_world_size();
        for level in self.all_levels_mut() {
            let border = level.get_world_border();
            border.set_absolute_max_size(max);
            border.take_events();
            // `this.raids = this.getDataStorage().computeIfAbsent(Raids.TYPE)` in the `ServerLevel` constructor
            level.get_data_storage().compute_if_absent(&crate::world::level::saveddata::raids::Raids::type_());
        }
    }

    /// `ServerLevel.getWorldBorder().isWithinBounds(pos)` for a level in the map.
    pub fn level_world_border_contains(&self, dimension: &DimensionKey, pos: BlockPos) -> bool {
        self.level(dimension).is_none_or(|l| l.is_within_world_border(pos))
    }
}

impl ServerLevel {
    /// `Level.getWorldBorderAdjustedRespawnData(respawnData)`: a respawn outside the border moves to the
    /// `MOTION_BLOCKING` height at the border centre.
    pub fn get_world_border_adjusted_respawn_data(&mut self, respawn_data: mc_world::level_storage::RespawnData) -> mc_world::level_storage::RespawnData {
        let (center_x, center_z) = {
            let border = self.get_world_border();
            if border.is_within_bounds_block(&respawn_data.pos) {
                return respawn_data;
            }
            (border.get_center_x(), border.get_center_z())
        };
        let center = BlockPos::containing(center_x, 0.0, center_z);
        // vanilla loads the chunk synchronously
        self.chunk_blocking_mut(center.x() >> 4, center.z() >> 4);
        let new_pos = self.get_heightmap_pos(mc_world::heightmap::HeightmapType::MotionBlocking, center);
        crate::world::entity::respawn::respawn_data_of(respawn_data.dimension, new_pos, respawn_data.yaw, respawn_data.pitch)
    }

    /// The border if it was already initialized by `get_world_border` (`createLevels` does that for every level).
    pub fn world_border_ref(&self) -> Option<&WorldBorder> {
        self.chunk_source.data_storage.peek(&WorldBorder::type_())
    }

    /// Mutable access without applying the initial settings.
    pub fn world_border_mut(&mut self) -> Option<&mut WorldBorder> {
        self.chunk_source.data_storage.get(&WorldBorder::type_())
    }

    /// `getWorldBorder().isWithinBounds(chunkPos)`.
    pub fn is_within_world_border_chunk(&self, pos: mc_core::pos::ChunkPos) -> bool {
        self.world_border_ref().is_none_or(|b| b.is_within_bounds_chunk(&pos))
    }

    /// `getWorldBorder().isWithinBounds(pos)`.
    pub fn is_within_world_border(&self, pos: BlockPos) -> bool {
        self.world_border_ref().is_none_or(|b| b.is_within_bounds_block(&pos))
    }
}
