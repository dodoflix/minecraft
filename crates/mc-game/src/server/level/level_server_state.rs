//! `ServerLevel`'s access to server-global state: the scoreboard, the player roster (`PlayerList` broadcasts) and the
//! world border / difficulty accessors. The server owns the `MinecraftServer` object, but a level is taken out of it
//! while it ticks, so everything a level (and its entities) read or send through the server is a shared handle that the
//! server installs when it creates the level.
//!
//! Game rules: `ServerLevel::<rule_name>()` accessors over `tick_state.rules` (`LevelRules`).

use super::ServerLevel;
use crate::server::players::PlayerHandle;
use crate::world::entity::EntityId;
use crate::world::level::saveddata::world_border::WorldBorder;
use crate::world::scores::ServerScoreboard;
use mc_protocol::packet::Packet;
use mc_world::level_storage::Difficulty;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::sync::{Arc, LazyLock};

/// `MinecraftServer.getScoreboard()`, shared between the server and its levels.
#[derive(Clone)]
pub struct SharedScoreboard(Arc<RwLock<ServerScoreboard>>);

impl Default for SharedScoreboard {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(ServerScoreboard::new_server())))
    }
}

impl SharedScoreboard {
    pub fn read(&self) -> RwLockReadGuard<'_, ServerScoreboard> {
        self.0.read()
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, ServerScoreboard> {
        self.0.write()
    }
}

/// `PlayerList.players`: every player of every dimension (their connections), kept in sync by the player list.
#[derive(Clone, Default)]
pub struct PlayerRoster(Arc<RwLock<Vec<PlayerHandle>>>);

impl PlayerRoster {
    pub(crate) fn add(&self, handle: PlayerHandle) {
        self.0.write().push(handle);
    }

    pub(crate) fn remove(&self, handle: &PlayerHandle) {
        self.0.write().retain(|p| !p.same_player(handle));
    }

    pub fn players(&self) -> Vec<PlayerHandle> {
        self.0.read().clone()
    }
}

/// What the server installs on a level at creation.
#[derive(Clone, Default)]
pub struct LevelServerState {
    pub scoreboard: SharedScoreboard,
    pub roster: PlayerRoster,
}

static DEFAULT_BORDER: LazyLock<WorldBorder> = LazyLock::new(WorldBorder::default);

impl ServerLevel {
    /// `getDifficulty()`.
    pub fn difficulty(&self) -> Difficulty {
        self.level_data.get_difficulty()
    }

    /// `getLevelData().isHardcore()`.
    pub fn is_hardcore(&self) -> bool {
        self.level_data.is_hardcore()
    }

    /// `getScoreboard()` (read access).
    pub fn scoreboard(&self) -> RwLockReadGuard<'_, ServerScoreboard> {
        self.server_state.scoreboard.read()
    }

    /// `getScoreboard()` (mutation: scores, teams; the server flushes the queued packets each tick).
    pub fn scoreboard_mut(&self) -> RwLockWriteGuard<'_, ServerScoreboard> {
        self.server_state.scoreboard.write()
    }

    /// `getWorldBorder()`: the border saved data of this level (a default border before it exists).
    pub fn world_border(&self) -> &WorldBorder {
        self.world_border_ref().unwrap_or(&DEFAULT_BORDER)
    }

    /// `server.getPlayerList().getPlayers()` (every dimension).
    pub fn all_player_handles(&self) -> Vec<PlayerHandle> {
        self.server_state.roster.players()
    }

    /// `ServerPlayer.connection.send(packet)` for a player (any dimension) by entity id.
    pub fn send_to_player<P: Packet>(&self, id: EntityId, packet: &P) {
        if let Some(p) = self.server_state.roster.0.read().iter().find(|p| p.id == id) {
            p.connection.send(packet);
        }
    }

    /// `PlayerList.broadcastAll(packet)`: every player of every dimension.
    pub fn broadcast_to_all_players<P: Packet>(&self, packet: &P) {
        for p in self.server_state.roster.0.read().iter() {
            p.connection.send(packet);
        }
    }

    /// `PlayerList.broadcastAll(packet, dimension)`: the players of this level's dimension.
    pub fn broadcast_to_dimension_players<P: Packet>(&self, packet: &P) {
        for p in self.server_state.roster.0.read().iter() {
            if p.dimension() == self.dimension {
                p.connection.send(packet);
            }
        }
    }
}
