//! `ServerLevel.getWaypointManager()` and the entity hooks that feed it (`EntityCallbacks`, `Entity.setPosRaw`,
//! `LivingEntity.onAttributeUpdated`, `ServerPlayer.onAttributeUpdated`).
//!
//! Entities are taken out of the level while they tick, so the hooks that vanilla runs from inside entity code set
//! state on the entity (`EntityData.waypoint_dirty`, the attribute values) and [`ServerLevel::process_waypoint_state`]
//! applies it when the entity is back in storage (`put_back_entity` / `sync_entity`).

use super::ServerLevel;
use crate::server::waypoints::ServerWaypointManager;
use crate::world::entity::{EntityId, ServerPlayer};
use crate::world::waypoints::{
    ReceiverView, TEAM_BLACK_ICON_COLOR, TransmitterView, WaypointContext,
};
use mc_core::pos::ChunkPos;
use mc_data::Attribute;
use mc_protocol::packets::play::ClientboundTrackedWaypointPacket;

/// [`WaypointContext`] over the level (the manager itself is moved out of the level while it runs), and the player a respawn
/// took out of the level, if any (it still holds the connection).
struct LevelContext<'a>(&'a ServerLevel, Option<&'a ServerPlayer>);

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
        self.0
            .chunk_source
            .chunk_map
            .player_chunk_tracking_view(receiver)
            .is_in_view_distance(chunk.x, chunk.z)
    }

    fn has_indirect_passenger(&self, source: i32, receiver: i32) -> bool {
        match (self.0.get_entity(source), self.0.get_entity(receiver)) {
            (Some(source), Some(receiver)) => source.has_indirect_passenger(self.0, receiver),
            _ => false,
        }
    }

    fn send(&self, receiver: i32, packet: ClientboundTrackedWaypointPacket) {
        if let Some(player) = self.1.filter(|p| p.base_id() == receiver) {
            player.send(&packet);
        } else if let Some(player) = self
            .0
            .get_entity(receiver)
            .and_then(|e| e.as_any().downcast_ref::<ServerPlayer>())
        {
            player.send(&packet);
        }
    }

    fn locator_bar_enabled(&self) -> bool {
        self.0.locator_bar()
    }
}

impl ServerLevel {
    /// `getWaypointManager()`: runs `f` on the manager with the level as its context.
    pub fn with_waypoint_manager<R>(
        &mut self,
        f: impl FnOnce(&mut ServerWaypointManager, &dyn WaypointContext) -> R,
    ) -> R {
        let mut manager = std::mem::take(&mut self.waypoint_manager);
        let result = f(&mut manager, &LevelContext(self, None));
        self.waypoint_manager = manager;
        result
    }

    /// What a connection reads off the `LivingEntity` `id`.
    fn transmitter_view(&self, id: EntityId) -> Option<TransmitterView> {
        let e = self.get_entity(id)?;
        let living = e.living()?;
        let team_color = crate::world::entity::team::get_team(self, &self.scoreboard(), e)
            .and_then(|team| team.get_color())
            .map(|color| {
                if color == crate::world::scores::TeamColor::Black {
                    TEAM_BLACK_ICON_COLOR
                } else {
                    color.rgb()
                }
            });
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
        let Some(e) = self.get_entity(id) else {
            return (false, false);
        };
        let Some(living) = e.living() else {
            return (false, false);
        };
        let transmitting = living.get_attribute_value(Attribute::WAYPOINT_TRANSMIT_RANGE) > 0.0;
        let receiving =
            e.is_player() && living.get_attribute_value(Attribute::WAYPOINT_RECEIVE_RANGE) > 0.0;
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

    /// `EntityCallbacks.onTrackingEnd`/`onDestroyed` (`LivingEntity.onRemoval`) for the waypoint manager. A killed player that
    /// stays connected is only a transmitter that ends here: as a receiver it stays registered until the respawn removes it
    /// ([`ServerLevel::waypoint_remove_taken_player`]).
    pub fn waypoint_tracking_end(&mut self, id: EntityId) {
        let parked = self
            .entity_manager
            .get_removed(id)
            .is_some_and(super::server_level_entities::is_parked_dead_player);
        self.with_waypoint_manager(|m, ctx| {
            if m.has_player(id) && !parked {
                m.remove_player(ctx, id);
            } else if m.is_tracked(id) {
                m.untrack_waypoint(ctx, id);
            }
        });
    }

    /// `removePlayer(player)` of a player that its owner took out of the level (a respawn): the connections disconnect through
    /// that player's own connection, which a lookup of the level would not find.
    pub fn waypoint_remove_taken_player(&mut self, player: &ServerPlayer) {
        let id = player.base_id();
        let mut manager = std::mem::take(&mut self.waypoint_manager);
        if manager.has_player(id) {
            manager.remove_player(&LevelContext(self, Some(player)), id);
        }
        self.waypoint_manager = manager;
    }

    /// Applies what entity code changed since the entity was last in storage: the waypoint range attributes that
    /// `refreshDirtyAttributes` visited (`onAttributeUpdated`, each one alone like vanilla runs it) and a position change
    /// (`setPosRaw`). Like vanilla, nothing else tracks or untracks: a `/waypoint modify` keeps a tracked entity that does not
    /// transmit.
    pub fn process_waypoint_state(&mut self, id: EntityId) {
        let Some(e) = self.get_entity(id) else { return };
        if e.living().is_none() {
            return;
        }
        let dirty = e.base().waypoint_dirty;
        let attribute_updates = e.base().waypoint_attribute_updates.clone();
        let has_connection = e
            .as_any()
            .downcast_ref::<ServerPlayer>()
            .is_some_and(|p| p.connection.is_some());
        let (transmitting, receiving) = self.waypoint_roles(id);
        if (dirty || !attribute_updates.is_empty())
            && let Some(e) = self.get_entity_mut(id)
        {
            e.base_mut().waypoint_dirty = false;
            e.base_mut().waypoint_attribute_updates.clear();
        }
        self.with_waypoint_manager(|m, ctx| {
            for attribute in attribute_updates {
                if attribute == Attribute::WAYPOINT_RECEIVE_RANGE {
                    // `ServerPlayer.onAttributeUpdated`: a positive range adds the player again (TRACK packets again)
                    if receiving {
                        m.add_player(ctx, id, transmitting);
                    } else {
                        m.remove_player(ctx, id);
                    }
                } else if transmitting {
                    // `LivingEntity.onAttributeUpdated`
                    m.track_waypoint(ctx, id);
                } else {
                    m.untrack_waypoint(ctx, id);
                }
            }
            // `setPosRaw`
            if dirty && transmitting {
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
