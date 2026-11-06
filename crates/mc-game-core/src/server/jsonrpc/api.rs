//! The server-side services the JSON-RPC management protocol drives: vanilla's `MinecraftApi` and its
//! `Minecraft*Service` interfaces folded into one object-safe trait. The RPC layer ([`super::methods`]) owns the
//! protocol logic (set/diff semantics, kicks, the `RPC Connection #n:` log lines); implementors only provide the
//! primitive reads and writes of the game state.
//!
//! Every method may be called from any thread. The layer wraps each incoming method in
//! [`ManagementApi::run_on_server_thread`], so implementors can touch game-thread state directly from inside the
//! other methods *when they are reached through the RPC layer* (and must marshal themselves otherwise).

use mc_text::Component;
use mc_world::level_storage::{Difficulty, GameType};
use uuid::Uuid;

use crate::server::permissions::PermissionLevel;
use crate::server::players::{
    IpBanListEntry, NameAndId, ServerOpListEntry, UserBanListEntry, UserWhiteListEntry,
};

/// `ClientInfo`: the numbered RPC connection a call came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientInfo {
    pub connection_id: i32,
}

/// A connected player as the RPC layer sees a `ServerPlayer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagementPlayer {
    pub profile: NameAndId,
    /// `ServerPlayer.getIpAddress()`.
    pub ip_address: String,
}

/// `GameRuleType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuleType {
    Int,
    Bool,
}

impl GameRuleType {
    /// `getSerializedName`.
    pub fn serialized_name(self) -> &'static str {
        match self {
            GameRuleType::Int => "integer",
            GameRuleType::Bool => "boolean",
        }
    }
}

pub use crate::server::notifications::GameRuleValue;

/// One available game rule with its current value (`GameRules.availableRules()` + `get`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRuleInfo {
    /// `GameRule.id()`, always with a namespace (`minecraft:advance_time`).
    pub id: String,
    pub rule_type: GameRuleType,
    pub value: GameRuleValue,
}

/// The server settings the `serversettings/*` methods read and write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingKey {
    AutoSave,
    Difficulty,
    EnforceWhitelist,
    UsingWhitelist,
    MaxPlayers,
    PauseWhenEmptySeconds,
    PlayerIdleTimeout,
    AllowFlight,
    SpawnProtectionRadius,
    Motd,
    ForceGameMode,
    GameMode,
    ViewDistance,
    SimulationDistance,
    AcceptsTransfers,
    StatusHeartbeatInterval,
    OperatorUserPermissions,
    HidesOnlinePlayers,
    RepliesToStatus,
    EntityBroadcastRangePercentage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingValue {
    Bool(bool),
    Int(i32),
    Str(String),
    Difficulty(Difficulty),
    GameType(GameType),
    /// `getOperatorUserPermissions().level()` / the level of the `LevelBasedPermissionSet` to apply.
    PermissionLevel(PermissionLevel),
}

/// `ServerStatus.Version.current()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerVersion {
    pub name: String,
    pub protocol: i32,
}

pub trait ManagementApi: Send + Sync {
    /// `notificationManager.server() != null`: the dedicated server object exists (methods other than
    /// `rpc.discover` are refused before that).
    fn server_attached(&self) -> bool;

    /// `MinecraftServerStateService.isReady`.
    fn is_ready(&self) -> bool;

    /// `ServerStatus.Version.current()`.
    fn version(&self) -> ServerVersion;

    /// `MinecraftExecutorService.submit(..).join()`: run `task` on the game thread and return after it finished.
    fn run_on_server_thread(&self, task: Box<dyn FnOnce() + Send + '_>);

    // --- allowlist ---------------------------------------------------------------------------------------------
    fn allowlist_entries(&self) -> Vec<UserWhiteListEntry>;
    fn allowlist_add(&self, entry: UserWhiteListEntry) -> bool;
    fn allowlist_remove(&self, user: &NameAndId);
    fn allowlist_clear(&self);
    /// `DedicatedServer.kickUnlistedPlayers`.
    fn kick_unlisted_players(&self);

    // --- bans --------------------------------------------------------------------------------------------------
    fn user_ban_entries(&self) -> Vec<UserBanListEntry>;
    fn add_user_ban(&self, ban: UserBanListEntry);
    fn remove_user_ban(&self, user: &NameAndId);
    fn clear_user_bans(&self);
    fn ip_ban_entries(&self) -> Vec<IpBanListEntry>;
    fn add_ip_ban(&self, ban: IpBanListEntry);
    fn remove_ip_ban(&self, ip: &str);
    fn clear_ip_bans(&self);

    // --- operators ---------------------------------------------------------------------------------------------
    fn operator_entries(&self) -> Vec<ServerOpListEntry>;
    /// `PlayerList.op(user, permissions, bypassesPlayerLimit)`; both optional, `None` = vanilla's defaults.
    fn op(
        &self,
        user: NameAndId,
        level: Option<PermissionLevel>,
        bypasses_player_limit: Option<bool>,
    );
    fn deop(&self, user: &NameAndId);
    fn clear_operators(&self);

    // --- players -----------------------------------------------------------------------------------------------
    fn players(&self) -> Vec<ManagementPlayer>;
    fn player_by_id(&self, id: Uuid) -> Option<ManagementPlayer>;
    fn player_by_name(&self, name: &str) -> Option<ManagementPlayer>;
    fn players_with_address(&self, ip: &str) -> Vec<ManagementPlayer>;
    /// `nameToIdCache.get(id)`.
    fn cached_user_by_id(&self, id: Uuid) -> Option<NameAndId>;
    /// `sessionService.fetchProfile(id, true)`.
    fn fetch_user_by_id(&self, id: Uuid) -> Option<NameAndId>;
    /// `nameToIdCache.get(name)`.
    fn fetch_user_by_name(&self, name: &str) -> Option<NameAndId>;
    /// `PlayerList.remove(player)`.
    fn remove_player(&self, id: Uuid);
    /// `player.connection.disconnect(reason)`.
    fn disconnect_player(&self, id: Uuid, reason: Component);

    // --- messages and state ------------------------------------------------------------------------------------
    /// `player.sendSystemMessage(message)` / `sendOverlayMessage(message)`.
    fn send_system_message(&self, players: &[Uuid], message: Component, overlay: bool);
    /// Send to every online player.
    fn broadcast_system_message(&self, message: Component, overlay: bool);
    /// `saveEverything(suppressLogs, flush, force)`.
    fn save_everything(&self, suppress_logs: bool, flush: bool, force: bool) -> bool;
    /// `MinecraftServer.halt(waitForShutdown)`.
    fn halt(&self, wait_for_shutdown: bool);

    // --- game rules --------------------------------------------------------------------------------------------
    fn game_rules(&self) -> Vec<GameRuleInfo>;
    /// Validate and apply `key = value` (`key` as sent, e.g. `minecraft:advance_time` or `advance_time`). `Err` is the
    /// decoding failure message (unknown rule, wrong value type, value out of range) reported as `Invalid params`.
    fn update_game_rule(&self, key: &str, value: GameRuleValue) -> Result<GameRuleInfo, String>;

    // --- settings ----------------------------------------------------------------------------------------------
    fn get_setting(&self, key: SettingKey) -> SettingValue;
    /// Apply a setting (types always match the key). `StatusHeartbeatInterval` goes through
    /// [`ManagementApi::set_status_heartbeat_interval`] instead.
    fn set_setting(&self, key: SettingKey, value: SettingValue);
    /// `DedicatedServer.setStatusHeartbeatInterval`: `false` when the interval could not be rescheduled.
    fn set_status_heartbeat_interval(&self, seconds: i32) -> bool;
}
