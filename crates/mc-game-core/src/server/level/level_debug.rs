//! `ServerLevel.debugSynchronizers()`: the level's [`LevelDebugSynchronizers`] run against the level they belong to, so
//! the calls that change them take them out of the level while they run.

use super::ServerLevel;
use crate::util::debug::subscriptions;
use crate::world::entity::EntityId;
use crate::world::entity::ai::village::poi::PoiRecord;
use crate::world::level::gameevent::PositionSource;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::DebugSubscription;
use mc_protocol::packet::Packet;
use mc_protocol::packets::play::{
    ClientboundDebugBlockValuePacket, ClientboundDebugEntityValuePacket,
    ClientboundDebugEventPacket, DebugGameEventListenerInfo, DebugSubscriptionEvent,
    DebugSubscriptionUpdate,
};

impl ServerLevel {
    /// `debugSynchronizers.hasAnySubscriberFor(subscription)`.
    pub fn debug_has_any_subscriber_for(&self, subscription: DebugSubscription) -> bool {
        self.debug_synchronizers
            .as_ref()
            .is_some_and(|s| s.has_any_subscriber_for(subscription))
    }

    /// `server.debugSubscribers().hasAnySubscriberFor(subscription)`.
    pub fn debug_server_has_any_subscriber_for(&self, subscription: DebugSubscription) -> bool {
        self.server_state
            .debug_subscribers
            .has_any_subscriber_for(subscription)
    }

    /// `player.debugSubscriptions().contains(subscription)`.
    fn debug_player_subscribed(&self, id: EntityId, subscription: DebugSubscription) -> bool {
        self.player_record(id).is_some_and(|record| {
            self.is_op_uuid(record.uuid)
                && self
                    .server_state
                    .roster
                    .requested_debug_subscription(record.uuid, subscription)
        })
    }

    /// `broadcastToTracking(trackedChunk, subscription, packet)`.
    fn debug_broadcast_to_tracking_chunk<P: Packet>(
        &self,
        tracked_chunk: ChunkPos,
        subscription: DebugSubscription,
        packet: &P,
    ) {
        for id in self
            .chunk_source
            .chunk_map
            .get_players(tracked_chunk, false)
        {
            if self.debug_player_subscribed(id, subscription)
                && let Some(connection) = self.player_record(id).and_then(|p| p.connection)
            {
                connection.send(packet);
            }
        }
    }

    /// `sendBlockValue(blockPos, subscription, value)`.
    pub fn debug_send_block_value(&self, pos: BlockPos, value: DebugSubscriptionEvent) {
        let subscription = value.subscription();
        if self.debug_has_any_subscriber_for(subscription) {
            self.debug_broadcast_to_tracking_chunk(
                ChunkPos::containing(&pos),
                subscription,
                &ClientboundDebugBlockValuePacket {
                    block_pos: pos,
                    update: DebugSubscriptionUpdate::Value(value),
                },
            );
        }
    }

    /// `sendEntityValue(entity, subscription, value)`.
    pub fn debug_send_entity_value(&self, entity: EntityId, value: DebugSubscriptionEvent) {
        let subscription = value.subscription();
        if self.debug_has_any_subscriber_for(subscription) {
            let payload =
                super::server_entity::encode_payload(&ClientboundDebugEntityValuePacket {
                    entity_id: entity,
                    update: DebugSubscriptionUpdate::Value(value),
                });
            self.entity_tracker
                .send_to_tracking_players_filtered(entity, &payload, |id| {
                    self.debug_player_subscribed(id, subscription)
                });
        }
    }

    /// `EuclideanGameEventListenerRegistry.sendDebugInfo(level, listener)`: a listener was registered.
    pub fn debug_send_game_event_listener_info(&self, source: PositionSource, radius: i32) {
        let subscription = subscriptions::game_event_listeners();
        if !self.debug_has_any_subscriber_for(subscription) {
            return;
        }
        let info = DebugSubscriptionEvent::GameEventListeners(DebugGameEventListenerInfo {
            listener_radius: radius,
        });
        match source {
            PositionSource::Block(pos) => self.debug_send_block_value(pos, info),
            PositionSource::Entity { uuid, .. } => {
                if let Some(entity) = self.entity_manager.id_by_uuid(uuid) {
                    self.debug_send_entity_value(entity, info);
                }
            }
        }
    }

    /// `broadcastEventToTracking(blockPos, subscription, value)`.
    pub fn debug_broadcast_event_to_tracking(&self, pos: BlockPos, value: DebugSubscriptionEvent) {
        let subscription = value.subscription();
        if self.debug_has_any_subscriber_for(subscription) {
            self.debug_broadcast_to_tracking_chunk(
                ChunkPos::containing(&pos),
                subscription,
                &ClientboundDebugEventPacket { event: value },
            );
        }
    }

    /// `debugSynchronizers.tick(server.debugSubscribers())`.
    pub(super) fn tick_debug_synchronizers(&mut self) {
        if let Some(mut synchronizers) = self.debug_synchronizers.take() {
            synchronizers.tick(self);
            self.debug_synchronizers = Some(synchronizers);
        }
    }

    /// `MinecraftServer.debugSubscribers.tick()` for a server that has this level only (the scenario replays): the
    /// subscriptions the operators of the player roster asked for.
    pub fn tick_debug_subscribers(&self) {
        self.server_state
            .debug_subscribers
            .tick(&self.server_state.roster.players(), |p| {
                self.is_op_uuid(p.uuid)
            });
    }

    /// `debugSynchronizers.registerChunk(chunk)`.
    pub(super) fn debug_register_chunk(&mut self, pos: ChunkPos) {
        if let Some(mut synchronizers) = self.debug_synchronizers.take() {
            synchronizers.register_chunk(self, pos);
            self.debug_synchronizers = Some(synchronizers);
        }
    }

    /// `debugSynchronizers.dropChunk(chunkPos)`.
    pub(super) fn debug_drop_chunk(&mut self, pos: ChunkPos) {
        if let Some(synchronizers) = self.debug_synchronizers.as_mut() {
            synchronizers.drop_chunk(pos);
        }
    }

    /// `debugSynchronizers.registerBlockEntity(blockEntity)`.
    pub(super) fn debug_register_block_entity(&mut self, pos: BlockPos) {
        if let Some(mut synchronizers) = self.debug_synchronizers.take() {
            synchronizers.register_block_entity(self, pos);
            self.debug_synchronizers = Some(synchronizers);
        }
    }

    /// `debugSynchronizers.dropBlockEntity(pos)`.
    pub(super) fn debug_drop_block_entity(&mut self, pos: BlockPos) {
        if let Some(mut synchronizers) = self.debug_synchronizers.take() {
            synchronizers.drop_block_entity(self, pos);
            self.debug_synchronizers = Some(synchronizers);
        }
    }

    /// `debugSynchronizers.startTrackingChunk(player, chunkPos)`: the chunk was sent to the player.
    pub fn debug_start_tracking_chunk(&self, player: EntityId, pos: ChunkPos) {
        if let Some(synchronizers) = &self.debug_synchronizers {
            synchronizers.start_tracking_chunk(self, player, pos);
        }
    }

    /// `debugSynchronizers.registerPoi(record)`.
    pub fn debug_register_poi(&self, record: &PoiRecord) {
        if let Some(synchronizers) = &self.debug_synchronizers {
            synchronizers.register_poi(self, record);
        }
    }

    /// `debugSynchronizers.updatePoi(pos)`: the ticket count of the point of interest changed.
    pub fn debug_update_poi(&self, pos: BlockPos) {
        if let Some(synchronizers) = &self.debug_synchronizers {
            synchronizers.update_poi(self, pos);
        }
    }

    /// `debugSynchronizers.dropPoi(pos)`.
    pub fn debug_drop_poi(&self, pos: BlockPos) {
        if let Some(synchronizers) = &self.debug_synchronizers {
            synchronizers.drop_poi(self, pos);
        }
    }
}
