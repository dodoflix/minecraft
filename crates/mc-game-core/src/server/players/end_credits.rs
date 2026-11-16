//! `ServerPlayer.showEndCredits()` and the detached-player store it needs.
//!
//! Vanilla removes the player from its level with `CHANGED_DIMENSION` but keeps the `ServerPlayer` object alive in the
//! player list and the connection until the client answers the poem with `PERFORM_RESPAWN`. Here an entity that is
//! removed from its level's storage is gone, so the real player moves into [`MinecraftServer::detached_players`]
//! (a removed placeholder with the same id goes through the level's removal events: trackers, chunk map, ...), and
//! `game_packets::with_player` / `respawn::respawn` find it there.

use crate::server::MinecraftServer;
use crate::server::level::ServerLevel;
use crate::world::entity::{RemovalReason, ServerPlayer};
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
            self.send(&ClientboundGameEventPacket {
                event: ClientboundGameEventPacket::WIN_GAME,
                param: 0.0,
            });
            self.seen_credits = true;
        }
    }
}

/// What has to happen once a player's packet handling / tick is over and the player is back in its level: the removal
/// `showEndCredits()` asked for.
pub fn finish_player_step(server: &mut MinecraftServer, uuid: Uuid) {
    let Some((id, dimension)) = server
        .player_list
        .get_player(&uuid)
        .map(|h| (h.id, h.dimension()))
    else {
        return;
    };
    server.with_level(&dimension, |server, level| {
        let Some(index) = level
            .tick_state
            .pending_player_detach
            .iter()
            .position(|&e| e == id)
        else {
            return;
        };
        level.tick_state.pending_player_detach.remove(index);
        // `removePlayerImmediately(CHANGED_DIMENSION)`: the removal `PlayerList.respawn` does of the old player
        if let Some(real) =
            super::respawn::take_old_player(level, id, RemovalReason::ChangedDimension)
        {
            let won_game = real.won_game;
            server.detached_players.insert(uuid, Box::new(real));
            if let Some(handle) = server.player_list.get_player(&uuid) {
                handle.shared.lock().won_game = won_game;
            }
        }
    });
}
