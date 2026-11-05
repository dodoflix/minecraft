//! `ServerLevel`'s access to server-global state: the scoreboard, the player roster (`PlayerList` broadcasts) and the
//! world border / difficulty accessors. The server owns the `MinecraftServer` object, but a level is taken out of it
//! while it ticks, so everything a level (and its entities) read or send through the server is a shared handle that the
//! server installs when it creates the level.
//!
//! Game rules: `ServerLevel::<rule_name>()` accessors over `server_state.rules` (`LiveRules`).

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
    pub fn add(&self, handle: PlayerHandle) {
        self.0.write().push(handle);
    }

    pub fn remove(&self, handle: &PlayerHandle) {
        self.0.write().retain(|p| !p.same_player(handle));
    }

    pub fn players(&self) -> Vec<PlayerHandle> {
        self.0.read().clone()
    }
}

/// The `DedicatedServer.isUnderSpawnProtection` inputs; the server refreshes them right before it ticks the level
/// (ops and the radius only change through commands/management, which run before the level ticks).
#[derive(Clone, Default)]
pub struct SpawnProtection {
    pub respawn: Option<mc_world::level_storage::RespawnData>,
    pub radius: i32,
    pub ops_empty: bool,
    /// Online players that are operators.
    pub op_players: Vec<uuid::Uuid>,
}

/// What the server installs on a level at creation.
#[derive(Clone, Default)]
pub struct LevelServerState {
    pub scoreboard: SharedScoreboard,
    pub roster: PlayerRoster,
    /// What loot evaluation reads from the server (random sequences, command storage, clocks).
    pub loot: crate::world::level::storage::loot::server_access::LootServices,
    pub spawn_protection: SpawnProtection,
    /// `getGameRules()`: shared with the server's `GameRules`, always current.
    pub rules: super::level_rules::LiveRules,
}

static DEFAULT_BORDER: LazyLock<WorldBorder> = LazyLock::new(WorldBorder::default);

impl ServerLevel {
    /// `DedicatedServer.isUnderSpawnProtection(level, pos, player)`.
    pub fn is_under_spawn_protection(&self, player: &crate::world::entity::ServerPlayer, pos: mc_core::pos::BlockPos) -> bool {
        self.is_under_spawn_protection_uuid(crate::world::entity::Entity::uuid(player), pos)
    }

    /// `PlayerList.isOp(player)`.
    pub fn is_op_uuid(&self, player: uuid::Uuid) -> bool {
        self.server_state.spawn_protection.op_players.contains(&player)
    }

    /// `isUnderSpawnProtection` for a player known by uuid (damage sources only remember their causing entity).
    pub fn is_under_spawn_protection_uuid(&self, player: uuid::Uuid, pos: mc_core::pos::BlockPos) -> bool {
        let sp = &self.server_state.spawn_protection;
        let Some(respawn) = &sp.respawn else { return false };
        if self.dimension != respawn.dimension || sp.ops_empty || sp.op_players.contains(&player) || sp.radius <= 0 {
            return false;
        }
        (pos.x() - respawn.pos.x()).abs().max((pos.z() - respawn.pos.z()).abs()) <= sp.radius
    }

    /// `ServerLevel.mayInteract(entity, pos)` for a player (by uuid): not under spawn protection and inside the world border.
    pub fn may_interact_player_uuid(&self, player: uuid::Uuid, pos: mc_core::pos::BlockPos) -> bool {
        !(self.is_under_spawn_protection_uuid(player, pos) || !self.world_border().is_within_bounds_block(&pos))
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::level::level_data::{PrimaryServerLevelData, ServerLevelData, default_world_data};

    /// The accessors read the shared server state at the point of use (vanilla: `/difficulty hard` is visible to the
    /// damage code of the same tick), not a per-tick copy.
    #[test]
    fn difficulty_is_read_live() {
        let world = default_world_data();
        let data = PrimaryServerLevelData(world.clone());
        world.lock().settings.difficulty_settings.difficulty = Difficulty::Hard;
        assert_eq!(data.get_difficulty(), Difficulty::Hard);
        world.lock().settings.difficulty_settings.difficulty = Difficulty::Peaceful;
        assert_eq!(data.get_difficulty(), Difficulty::Peaceful);
    }

    #[test]
    fn scoreboard_handle_is_shared() {
        let a = SharedScoreboard::default();
        let b = a.clone();
        a.write().add_player_team("red");
        assert_eq!(b.read().get_team_names().count(), 1);
    }
}
