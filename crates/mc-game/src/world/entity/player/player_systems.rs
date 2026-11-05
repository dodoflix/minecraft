//! The player part of the play-packet dispatcher's `PlayerSystems` (vanilla's `this.player.xxx(...)` calls of
//! `ServerGamePacketListenerImpl`). Exactly one implementor is used (`PLAYER_SYSTEMS.first()`), so every slice that owns
//! one of these methods adds its delegation here.

use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAYER_SYSTEMS, PlayerSystems};
use linkme::distributed_slice;

struct ServerPlayerSystems;

impl PlayerSystems for ServerPlayerSystems {
    /// `ServerPlayer.sendBuildLimitMessage(isTooHigh, limit)`.
    fn send_build_limit_message(&self, ctx: &mut PlayCtx, upper: bool, limit: i32) {
        ctx.player.send_build_limit_message(upper, limit);
    }

    /// `ServerPlayer.sendSpawnProtectionMessage(pos)`.
    fn send_spawn_protection_message(&self, ctx: &mut PlayCtx, pos: mc_core::pos::BlockPos) {
        ctx.player.send_spawn_protection_message(pos);
    }

    /// `ServerPlayer.setCamera(target)`.
    fn set_camera(&self, ctx: &mut PlayCtx, target: crate::world::entity::EntityId) {
        ctx.player.set_camera(ctx.level, Some(target));
    }

    /// `ServerPlayer.teleportTo(level, x, y, z, Set.of(), yRot, xRot, resetCamera)` within the player's own dimension
    /// (spectator `ServerboundTeleportToEntityPacket`); a target in another dimension needs the server-level
    /// `dimension_change::teleport_player`, which cannot run while the player is taken out of its level.
    fn teleport_to(&self, ctx: &mut PlayCtx, dimension: &crate::world::level::DimensionKey, x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32, set_camera: bool) {
        if *dimension != ctx.level.dimension {
            return;
        }
        if set_camera {
            ctx.player.set_camera(ctx.level, None);
        }
        ctx.player.connection_teleport(x, y, z, y_rot, x_rot);
        crate::world::entity::Entity::set_y_head_rot(&mut *ctx.player, y_rot);
        if let Some(l) = ctx.player.listener.as_mut() {
            l.reset_flying_ticks();
        }
    }

    /// `getStats().sendStats(player)`.
    fn send_stats(&self, ctx: &mut PlayCtx) {
        let packet = ctx.player.stats.lock().stats_packet();
        ctx.send(&packet);
    }

    /// `ServerPlayer.checkMovementStatistics(dx, dy, dz)`.
    fn check_movement_statistics(&self, ctx: &mut PlayCtx, dx: f64, dy: f64, dz: f64) {
        ctx.player.check_movement_statistics(ctx.level, dx, dy, dz);
    }
}

#[distributed_slice(PLAYER_SYSTEMS)]
static SERVER_PLAYER_SYSTEMS: &dyn PlayerSystems = &ServerPlayerSystems;
