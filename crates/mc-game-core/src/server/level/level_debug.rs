//! `ServerLevel.debugSynchronizers()`: the level's [`LevelDebugSynchronizers`] run against the level they belong to, so
//! the calls that change them take them out of the level while they run.

use super::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::ai::village::poi::PoiRecord;
use mc_core::pos::{BlockPos, ChunkPos};

impl ServerLevel {
    /// Tells the debug synchronizers what the entity tracker recorded (`registerEntity`, `startTrackingEntity`,
    /// `dropEntity` of `ChunkMap.TrackedEntity`); called after every tracker call.
    pub(super) fn apply_debug_tracker_events(&mut self) {
        let events = self.entity_tracker.take_events();
        if events.is_empty() {
            return;
        }
        if let Some(mut synchronizers) = self.debug_synchronizers.take() {
            synchronizers.apply_tracker_events(self, events);
            self.debug_synchronizers = Some(synchronizers);
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
