//! `ServerLevel.getWaypointManager()` and the entity hooks that feed it (`EntityCallbacks`, `Entity.setPosRaw`,
//! `LivingEntity.onAttributeUpdated`, `ServerPlayer.onAttributeUpdated`).
//!
//! Entities are taken out of the level while they tick, so the hooks that vanilla runs from inside entity code set
//! state on the entity (`EntityData.waypoint_dirty`, the attribute values) and [`ServerLevel::process_waypoint_state`]
//! applies it when the entity is back in storage (`put_back_entity` / `sync_entity`).

use super::ServerLevel;
use crate::server::waypoints::ServerWaypointManager;
use crate::world::entity::{EntityId, ServerPlayer};
use crate::world::waypoints::{ReceiverView, TEAM_BLACK_ICON_COLOR, TransmitterView, WaypointContext};
use mc_core::pos::ChunkPos;
use mc_data::Attribute;
use mc_protocol::packets::play::ClientboundTrackedWaypointPacket;

/// [`WaypointContext`] over the level (the manager itself is moved out of the level while it runs).
struct LevelContext<'a>(&'a ServerLevel);

impl WaypointContext for LevelContext<'_> {
    fn transmitter(&self, id: i32) -> Option<TransmitterView> {
        self.0.transmitter_view(id)
    }

    fn receiver(&self, id: i32) -> Option<ReceiverView> {
        let e = self.0.get_entity(id)?;
        let living = e.living()?;
        Some(ReceiverView {
            id,
            position: e.position(),
            spectator: e.is_spectator(),
            receive_range: living.get_attribute_value(Attribute::WAYPOINT_RECEIVE_RANGE),
        })
    }

    fn is_chunk_visible(&self, receiver: i32, chunk: ChunkPos) -> bool {
        self.0.chunk_source.chunk_map.player_chunk_tracking_view(receiver).is_in_view_distance(chunk.x, chunk.z)
    }

    fn has_indirect_passenger(&self, source: i32, receiver: i32) -> bool {
        match (self.0.get_entity(source), self.0.get_entity(receiver)) {
            (Some(source), Some(receiver)) => source.has_indirect_passenger(self.0, receiver),
            _ => false,
        }
    }

    fn send(&self, receiver: i32, packet: ClientboundTrackedWaypointPacket) {
        if let Some(player) = self.0.get_entity(receiver).and_then(|e| e.as_any().downcast_ref::<ServerPlayer>()) {
            player.send(&packet);
        }
    }

    fn locator_bar_enabled(&self) -> bool {
        self.0.locator_bar()
    }
}

impl ServerLevel {
    /// `getWaypointManager()`: runs `f` on the manager with the level as its context.
    pub fn with_waypoint_manager<R>(&mut self, f: impl FnOnce(&mut ServerWaypointManager, &dyn WaypointContext) -> R) -> R {
        let mut manager = std::mem::take(&mut self.waypoint_manager);
        let result = f(&mut manager, &LevelContext(self));
        self.waypoint_manager = manager;
        result
    }

    /// What a connection reads off the `LivingEntity` `id`.
    fn transmitter_view(&self, id: EntityId) -> Option<TransmitterView> {
        let e = self.get_entity(id)?;
        let living = e.living()?;
        let team_color = self
            .scoreboard()
            .get_players_team(&e.get_scoreboard_name())
            .and_then(|team| team.get_color())
            .map(|color| if color == crate::world::scores::TeamColor::Black { TEAM_BLACK_ICON_COLOR } else { color.rgb() });
        Some(TransmitterView {
            id,
            uuid: e.uuid(),
            position: e.position(),
            block_position: e.block_position(),
            chunk_position: e.base().chunk_position(),
            first_tick: e.base().first_tick,
            spectator: e.is_spectator(),
            transmit_range: living.get_attribute_value(Attribute::WAYPOINT_TRANSMIT_RANGE),
            icon: living.locator_bar_icon.clone(),
            team_color,
        })
    }

    /// `isTransmittingWaypoint()` / `isReceivingWaypoints()` of the entity `id`: `(transmitting, receiving player)`.
    fn waypoint_roles(&self, id: EntityId) -> (bool, bool) {
        let Some(e) = self.get_entity(id) else { return (false, false) };
        let Some(living) = e.living() else { return (false, false) };
        let transmitting = living.get_attribute_value(Attribute::WAYPOINT_TRANSMIT_RANGE) > 0.0;
        let receiving = e.is_player() && living.get_attribute_value(Attribute::WAYPOINT_RECEIVE_RANGE) > 0.0;
        (transmitting, receiving)
    }

    /// `EntityCallbacks.onTrackingStart`/`onCreated` for the waypoint manager.
    pub fn waypoint_tracking_start(&mut self, id: EntityId) {
        let (transmitting, receiving) = self.waypoint_roles(id);
        if receiving {
            self.with_waypoint_manager(|m, ctx| m.add_player(ctx, id, transmitting));
        } else if transmitting {
            self.with_waypoint_manager(|m, ctx| m.track_waypoint(ctx, id));
        }
    }

    /// `EntityCallbacks.onTrackingEnd`/`onDestroyed` (`LivingEntity.onRemoval`) for the waypoint manager.
    pub fn waypoint_tracking_end(&mut self, id: EntityId) {
        self.with_waypoint_manager(|m, ctx| {
            if m.has_player(id) {
                m.remove_player(ctx, id);
            } else if m.is_tracked(id) {
                m.untrack_waypoint(ctx, id);
            }
        });
    }

    /// Applies what entity code changed since the entity was last in storage: a transmit/receive range that crossed zero
    /// (`onAttributeUpdated`) and a position change (`setPosRaw`). shortcut: a range change that stays above zero does not
    /// re-send the track packets like vanilla's `trackWaypoint` does.
    pub fn process_waypoint_state(&mut self, id: EntityId) {
        let Some(e) = self.get_entity(id) else { return };
        if e.living().is_none() {
            return;
        }
        let dirty = e.base().waypoint_dirty;
        let has_connection = e.as_any().downcast_ref::<ServerPlayer>().is_some_and(|p| p.connection.is_some());
        let (transmitting, receiving) = self.waypoint_roles(id);
        if dirty {
            if let Some(e) = self.get_entity_mut(id) {
                e.base_mut().waypoint_dirty = false;
            }
        }
        self.with_waypoint_manager(|m, ctx| {
            if receiving && !m.has_player(id) {
                m.add_player(ctx, id, transmitting);
            } else if !receiving && m.has_player(id) {
                m.remove_player(ctx, id);
            }
            if transmitting && !m.is_tracked(id) {
                m.track_waypoint(ctx, id);
            } else if !transmitting && m.is_tracked(id) {
                m.untrack_waypoint(ctx, id);
            } else if dirty && transmitting {
                m.update_waypoint(ctx, id);
            }
            if dirty && receiving && has_connection {
                m.update_player(ctx, id);
            }
        });
    }

    /// `LOCATOR_BAR` changed (`MinecraftServer.onGameRuleChanged`): `locator_bar` is the new value.
    pub fn waypoint_locator_bar_changed(&mut self, locator_bar: bool) {
        let players: Vec<EntityId> = self.players().to_vec();
        self.with_waypoint_manager(|m, ctx| {
            if locator_bar {
                for player in players {
                    m.update_player(ctx, player);
                }
            } else {
                m.break_all_connections(ctx);
            }
        });
    }

    /// `ServerScoreboard.updatePlayerWaypoint`: `getWaypointManager().remakeConnections(entity)`.
    pub fn waypoint_remake_connections(&mut self, id: EntityId) {
        self.with_waypoint_manager(|m, ctx| m.remake_connections(ctx, id));
    }
}
