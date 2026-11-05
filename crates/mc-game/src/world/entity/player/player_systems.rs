//! The player part of the play-packet dispatcher's `PlayerSystems` (vanilla's `this.player.xxx(...)` calls of
//! `ServerGamePacketListenerImpl`). Exactly one implementor is used (`PLAYER_SYSTEMS.first()`), so every slice that owns
//! one of these methods adds its delegation here.

use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAYER_SYSTEMS, PlayerSystems};
use linkme::distributed_slice;

struct ServerPlayerSystems;

impl PlayerSystems for ServerPlayerSystems {
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
