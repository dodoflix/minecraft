//! `ServerPlayer.camera`: `getCamera`/`setCamera`/the camera part of `tick()` (spectator mode follows an entity's view),
//! and `ServerChunkCache.move(player)`.

use super::ServerLevel;
use super::chunk_map::PlayerTrackingUpdate;
use crate::world::entity::{Entity, EntityId, ServerPlayer};
use mc_protocol::packets::play::ClientboundSetCameraPacket;

impl ServerPlayer {
    /// `getCamera()`: the entity whose view the client shows (the player itself unless spectating another entity).
    pub fn get_camera(&self) -> EntityId {
        self.camera.unwrap_or_else(|| self.base_id())
    }

    /// `level.getChunkSource().move(player)`: the tracker half (with the previous chunk tracking view), then the chunk
    /// map's player position / chunk tracking.
    pub fn chunk_source_move(&mut self, level: &mut ServerLevel) {
        let update = PlayerTrackingUpdate {
            entity_id: self.base_id(),
            position: self.position(),
            requested_view_distance: self.requested_view_distance,
            spectator: self.is_spectator(),
            alive: self.is_alive(),
        };
        level.entity_tracker_move_taken_out_player(self);
        level.chunk_source.chunk_map.move_player(update);
    }

    /// `setCamera(newCamera)`: `None` (or the player itself) resets the camera. The player is teleported to the camera
    /// entity (`teleportTo(level, x, y, z, Set.of(), yRot, xRot, false)`), the chunk source follows, and the client is told.
    pub fn set_camera(&mut self, level: &mut ServerLevel, new_camera: Option<EntityId>) {
        let own = self.base_id();
        let old_camera = self.get_camera();
        self.camera = new_camera.filter(|&c| c != own);
        let camera = self.get_camera();
        if old_camera == camera {
            return;
        }
        // `camera.level() instanceof ServerLevel`: an entity in another dimension is not found here
        let target = if camera == own { Some(self.position()) } else { level.get_entity(camera).map(|e| e.position()) };
        if let Some(pos) = target {
            let (y_rot, x_rot) = (self.base().y_rot(), self.base().x_rot());
            if self.player.living.is_sleeping() {
                // SEAM(EcPlayer): `stopSleepInBed(true, true)` of `ServerPlayer.teleportTo`
            }
            self.connection_teleport(pos.x, pos.y, pos.z, y_rot, x_rot);
            let head = y_rot;
            self.set_y_head_rot(head);
            if let Some(l) = self.listener.as_mut() {
                l.reset_flying_ticks();
            }
        }
        if new_camera.is_some() {
            self.chunk_source_move(level);
        }
        self.send(&ClientboundSetCameraPacket { camera_id: camera });
        self.reset_position_after_teleport();
    }

    /// The camera part of `ServerPlayer.tick()`: follow a living camera entity, drop a dead one.
    pub fn tick_camera(&mut self, level: &mut ServerLevel) {
        let camera = self.get_camera();
        if camera == self.base_id() {
            return;
        }
        let snap = level.get_entity(camera).filter(|e| e.is_alive()).map(|e| (e.position(), e.base().y_rot(), e.base().x_rot()));
        match snap {
            Some((pos, y_rot, x_rot)) => {
                self.abs_snap_to(pos.x, pos.y, pos.z, y_rot, x_rot);
                self.chunk_source_move(level);
                if self.is_shift_key_down() {
                    self.set_camera(level, None);
                }
            }
            None => self.set_camera(level, None),
        }
    }
}
