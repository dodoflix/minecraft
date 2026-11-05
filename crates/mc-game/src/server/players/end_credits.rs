//! `ServerPlayer.showEndCredits()` and the detached-player store it needs.
//!
//! Vanilla removes the player from its level with `CHANGED_DIMENSION` but keeps the `ServerPlayer` object alive in the
//! player list and the connection until the client answers the poem with `PERFORM_RESPAWN`. Here an entity that is
//! removed from its level's storage is gone, so the real player moves into [`MinecraftServer::detached_players`]
//! (a removed placeholder with the same id goes through the level's removal events: trackers, chunk map, ...), and
//! `game_packets::with_player` / `respawn::respawn` find it there.

use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::world::entity::{Entity, RemovalReason, ServerPlayer};
use mc_protocol::packets::play::ClientboundGameEventPacket;
use uuid::Uuid;

impl ServerPlayer {
    /// `showEndCredits()`: `unRide()`, `removePlayerImmediately(CHANGED_DIMENSION)` (applied at the end of the current
    /// player step by [`finish_player_step`]), then `WIN_GAME` once.
    pub fn show_end_credits(&mut self, level: &mut ServerLevel) {
        level.un_ride(self);
        let id = self.base_id();
        if !level.tick_state.pending_player_detach.contains(&id) {
            level.tick_state.pending_player_detach.push(id);
        }
        if !self.won_game {
            self.won_game = true;
            self.send(&ClientboundGameEventPacket { event: ClientboundGameEventPacket::WIN_GAME, param: 0.0 });
            self.seen_credits = true;
        }
    }
}

/// What has to happen once a player's packet handling / tick is over and the player is back in its level: the portal
/// teleport `handlePortal()` decided on, the removal `showEndCredits()` asked for.
pub fn finish_player_step(server: &mut MinecraftServer, uuid: Uuid) {
    let Some((id, dimension)) = server.player_list.get_player(&uuid).map(|h| (h.id, h.dimension())) else { return };
    let moved_to = server
        .with_level(&dimension, |server, level| {
            if level.take_portal_teleport(id) {
                level.with_entity(id, |level, e| crate::server::players::dimension_change::teleport_through_portal(server, level, e));
                return server.player_list.get_player(&uuid).map(|h| h.dimension()).filter(|d| *d != dimension);
            }
            if let Some(index) = level.tick_state.pending_player_detach.iter().position(|&e| e == id) {
                level.tick_state.pending_player_detach.remove(index);
                level.with_entity(id, |level, e| {
                    let Some(slot) = e.as_any_mut().downcast_mut::<ServerPlayer>() else { return };
                    let mut placeholder = ServerPlayer::new(id, slot.dimension.clone(), slot.player.game_profile.clone(), Default::default());
                    placeholder.remove(RemovalReason::ChangedDimension);
                    let mut real = Box::new(std::mem::replace(slot, placeholder));
                    real.close_menus_on_removal(level);
                    real.remove(RemovalReason::ChangedDimension);
                    server.detached_players.insert(uuid, real);
                });
            }
            None
        })
        .flatten();
    // a player that changed dimension runs the tail of `doTick()` in its new level (see `teleport_player_through_portal`)
    if let Some(new_dimension) = moved_to {
        server.with_level(&new_dimension, |_, level| level.with_player(id, |level, player| player.do_tick_tail(level)));
    }
}
