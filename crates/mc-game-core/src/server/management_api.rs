//! `ManagementApi` (the JSON-RPC management protocol's view of the server) and `RconCommandRunner` over the running
//! [`MinecraftServer`]. Both live on other threads than the game thread, so they hold only an `Arc<ServerShared>` and run
//! everything on the game thread (`MinecraftExecutorService`/`DedicatedServer.runCommand`'s `executeBlocking`).
//!
//! The RPC layer wraps every method in [`ManagementApi::run_on_server_thread`]; inside it the server is reachable through
//! the command system's `server_access` (the same installed-pointer mechanism commands use), so the primitive methods
//! below find it with `on_server` and only marshal to the game thread themselves when called from anywhere else.

use super::MinecraftServer;
use super::jsonrpc::ManagementApi;
use super::jsonrpc::api::{
    GameRuleInfo, GameRuleType, ManagementPlayer, ServerVersion, SettingKey, SettingValue,
};
use super::notifications::GameRuleValue as NotifyValue;
use super::permissions::{LevelBasedPermissionSet, PermissionLevel};
use super::players::{
    IpBanListEntry, NameAndId, ServerOpListEntry, UserBanListEntry, UserWhiteListEntry,
};
use super::rcon::RconCommandRunner;
use super::shared::ServerShared;
use crate::commands::console_command_source::RconConsoleSource;
use crate::commands::server_access::{enter_server, has_server, with_server};
use crate::world::level::gamerules::{AnyGameRule, GameRuleType as RuleType, GameRuleValue};
use mc_text::Component;
use mc_world::level_storage::GameType;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

/// Run `f` on the game thread with the server. Inside `run_on_server_thread` (server installed) it runs inline.
fn on_server<R: Send + 'static>(
    shared: &ServerShared,
    f: impl FnOnce(&mut MinecraftServer) -> R + Send,
) -> R {
    if has_server() {
        return with_server(f);
    }
    let task: Box<dyn FnOnce(&mut MinecraftServer) -> R + Send + '_> = Box::new(f);
    // SAFETY: `execute_blocking` does not return before the task finished (or was dropped unrun after the game thread
    // exited, in which case the closure is dropped on that thread before we return), so the borrows `f` captures
    // outlive its use; only the `'static` bound of the queue is being circumvented.
    let task: Box<dyn FnOnce(&mut MinecraftServer) -> R + Send + 'static> =
        unsafe { std::mem::transmute(task) };
    shared
        .execute_blocking(task)
        .unwrap_or_else(|| panic!("the server is not running"))
}

pub struct ServerManagementApi {
    shared: Arc<ServerShared>,
}

impl ServerManagementApi {
    pub fn new(shared: Arc<ServerShared>) -> Self {
        Self { shared }
    }

    fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut MinecraftServer) -> R + Send) -> R {
        on_server(&self.shared, f)
    }
}

fn management_player(handle: &super::players::player_handle::PlayerHandle) -> ManagementPlayer {
    ManagementPlayer {
        profile: handle.name_and_id(),
        ip_address: handle.ip_address.clone(),
    }
}

fn rule_info(rules: &crate::world::level::gamerules::GameRules, rule: AnyGameRule) -> GameRuleInfo {
    let value = rules.get_value(rule);
    GameRuleInfo {
        id: rule.id(),
        rule_type: match rule.game_rule_type() {
            RuleType::Int => GameRuleType::Int,
            RuleType::Bool => GameRuleType::Bool,
        },
        value: match value {
            GameRuleValue::Bool(b) => NotifyValue::Bool(b),
            GameRuleValue::Int(i) => NotifyValue::Int(i),
        },
    }
}

impl ManagementApi for ServerManagementApi {
    fn server_attached(&self) -> bool {
        true
    }

    fn is_ready(&self) -> bool {
        self.shared.is_ready()
    }

    fn version(&self) -> ServerVersion {
        ServerVersion {
            name: mc_world::level_storage::WORLD_VERSION_NAME.to_owned(),
            protocol: mc_protocol::PROTOCOL_VERSION,
        }
    }

    fn run_on_server_thread(&self, task: Box<dyn FnOnce() + Send + '_>) {
        if has_server() {
            task();
            return;
        }
        // SAFETY: see `on_server`.
        let task: Box<dyn FnOnce() + Send + 'static> = unsafe { std::mem::transmute(task) };
        self.shared
            .execute_blocking(move |server| enter_server(server, task));
    }

    fn allowlist_entries(&self) -> Vec<UserWhiteListEntry> {
        self.with(|s| s.player_list.whitelist.get_entries().cloned().collect())
    }
    fn allowlist_add(&self, entry: UserWhiteListEntry) -> bool {
        self.with(|s| s.player_list.whitelist.add(entry))
    }
    fn allowlist_remove(&self, user: &NameAndId) {
        let user = user.clone();
        self.with(move |s| {
            s.player_list.whitelist.remove(&user);
        });
    }
    fn allowlist_clear(&self) {
        self.with(|s| s.player_list.whitelist.clear());
    }
    fn kick_unlisted_players(&self) {
        self.with(|s| s.kick_unlisted_players());
    }

    fn user_ban_entries(&self) -> Vec<UserBanListEntry> {
        self.with(|s| s.player_list.bans.get_entries().cloned().collect())
    }
    fn add_user_ban(&self, ban: UserBanListEntry) {
        self.with(|s| {
            s.player_list.bans.add(ban);
        });
    }
    fn remove_user_ban(&self, user: &NameAndId) {
        let user = user.clone();
        self.with(move |s| {
            s.player_list.bans.remove(&user);
        });
    }
    fn clear_user_bans(&self) {
        self.with(|s| s.player_list.bans.clear());
    }
    fn ip_ban_entries(&self) -> Vec<IpBanListEntry> {
        self.with(|s| s.player_list.ip_bans.get_entries().cloned().collect())
    }
    fn add_ip_ban(&self, ban: IpBanListEntry) {
        self.with(|s| {
            s.player_list.ip_bans.add(ban);
        });
    }
    fn remove_ip_ban(&self, ip: &str) {
        let ip = ip.to_owned();
        self.with(move |s| {
            s.player_list.ip_bans.remove(&ip);
        });
    }
    fn clear_ip_bans(&self) {
        self.with(|s| s.player_list.ip_bans.clear());
    }

    fn operator_entries(&self) -> Vec<ServerOpListEntry> {
        self.with(|s| s.player_list.ops.get_entries().cloned().collect())
    }
    fn op(
        &self,
        user: NameAndId,
        level: Option<PermissionLevel>,
        bypasses_player_limit: Option<bool>,
    ) {
        self.with(move |s| {
            super::players::player_ops::op(
                s,
                user,
                level.map(LevelBasedPermissionSet::for_level),
                bypasses_player_limit,
            );
        });
    }
    fn deop(&self, user: &NameAndId) {
        let user = user.clone();
        self.with(move |s| super::players::player_ops::deop(s, &user));
    }
    fn clear_operators(&self) {
        self.with(|s| {
            s.player_list.ops.clear();
            s.player_list.refresh_player_permissions();
        });
    }

    fn players(&self) -> Vec<ManagementPlayer> {
        self.with(|s| {
            s.player_list
                .get_players()
                .iter()
                .map(management_player)
                .collect()
        })
    }
    fn player_by_id(&self, id: Uuid) -> Option<ManagementPlayer> {
        self.with(move |s| s.player_list.get_player(&id).map(management_player))
    }
    fn player_by_name(&self, name: &str) -> Option<ManagementPlayer> {
        let name = name.to_owned();
        self.with(move |s| {
            s.player_list
                .get_player_by_name(&name)
                .map(management_player)
        })
    }
    fn players_with_address(&self, ip: &str) -> Vec<ManagementPlayer> {
        let ip = ip.to_owned();
        self.with(move |s| {
            s.player_list
                .get_players_with_address(&ip)
                .iter()
                .map(management_player)
                .collect()
        })
    }
    fn cached_user_by_id(&self, id: Uuid) -> Option<NameAndId> {
        use super::players::UserNameToIdResolver;
        self.with(move |s| s.name_to_id_cache.get_by_id(id))
    }
    fn fetch_user_by_id(&self, _id: Uuid) -> Option<NameAndId> {
        // `sessionService.fetchProfile`: no Mojang session service in offline operation.
        None
    }
    fn fetch_user_by_name(&self, name: &str) -> Option<NameAndId> {
        use super::players::UserNameToIdResolver;
        let name = name.to_owned();
        self.with(move |s| s.name_to_id_cache.get_by_name(&name))
    }
    fn remove_player(&self, id: Uuid) {
        self.with(move |s| s.remove_player_by_id(id));
    }
    fn disconnect_player(&self, id: Uuid, reason: Component) {
        self.with(move |s| {
            if let Some(player) = s.player_list.get_player(&id) {
                player.disconnect(&reason);
            }
        });
    }

    fn send_system_message(&self, players: &[Uuid], message: Component, overlay: bool) {
        let players = players.to_vec();
        self.with(move |s| {
            for id in players {
                if let Some(player) = s.player_list.get_player(&id) {
                    player.send_system_message(&message, overlay);
                }
            }
        });
    }
    fn broadcast_system_message(&self, message: Component, overlay: bool) {
        self.with(move |s| {
            for player in s.player_list.get_players() {
                player.send_system_message(&message, overlay);
            }
        });
    }
    fn save_everything(&self, suppress_logs: bool, flush: bool, force: bool) -> bool {
        self.with(move |s| s.save_everything(suppress_logs, flush, force))
    }
    fn halt(&self, wait_for_shutdown: bool) {
        self.shared.halt();
        if wait_for_shutdown {
            while !self.shared.is_shutdown() {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }

    fn game_rules(&self) -> Vec<GameRuleInfo> {
        self.with(|s| {
            let rules = s.game_rules_ref();
            rules
                .available_rules()
                .map(|rule| rule_info(rules, rule))
                .collect()
        })
    }

    fn update_game_rule(&self, key: &str, value: NotifyValue) -> Result<GameRuleInfo, String> {
        let key = key.to_owned();
        self.with(move |s| {
            let id = if key.contains(':') {
                key.clone()
            } else {
                format!("minecraft:{key}")
            };
            let rule = mc_core::ident::Identifier::try_parse(&id)
                .and_then(|id| AnyGameRule::by_identifier(&id))
                .filter(|rule| s.game_rules_ref().available_rules().any(|r| r == *rule))
                .ok_or_else(|| format!("Unknown game rule: {key}"))?;
            let typed = match (rule.game_rule_type(), value) {
                (RuleType::Bool, NotifyValue::Bool(b)) => GameRuleValue::Bool(b),
                (RuleType::Int, NotifyValue::Int(i)) => {
                    let (min, max) = rule.int_range();
                    if i < min || i > max {
                        return Err(format!("Value {i} outside of range [{min}:{max}]"));
                    }
                    GameRuleValue::Int(i)
                }
                _ => return Err(format!("Invalid value type for game rule {key}")),
            };
            if s.game_rules().try_set_value(rule, typed) {
                s.on_game_rule_changed(rule, typed);
            }
            Ok(rule_info(s.game_rules_ref(), rule))
        })
    }

    fn get_setting(&self, key: SettingKey) -> SettingValue {
        self.with(move |s| s.get_management_setting(key))
    }
    fn set_setting(&self, key: SettingKey, value: SettingValue) {
        self.with(move |s| s.set_management_setting(key, value));
    }
    fn set_status_heartbeat_interval(&self, seconds: i32) -> bool {
        self.with(move |s| s.set_status_heartbeat_interval(seconds))
    }
}

impl MinecraftServer {
    /// `kickUnlistedPlayers`.
    pub fn kick_unlisted_players(&mut self) {
        if self.is_enforce_whitelist() && self.is_using_whitelist() {
            for player in self.player_list.get_players() {
                if !self.player_list.is_white_listed(&player.name_and_id()) {
                    player.disconnect(&Component::translatable(
                        "multiplayer.disconnect.not_whitelisted",
                        vec![],
                    ));
                }
            }
        }
    }

    /// `PlayerList.remove(player)` for the management API: the player leaves the level and the list.
    pub fn remove_player_by_id(&mut self, id: Uuid) {
        let Some(handle) = self.player_list.get_player(&id).cloned() else {
            return;
        };
        let (entity_id, dimension) = (handle.id, handle.dimension());
        self.with_level(&dimension, |server, level| {
            level.with_player(entity_id, |level, player| {
                super::players::player_ops::remove(server, level, player)
            });
        });
    }

    /// `DedicatedServer.setStatusHeartbeatInterval`.
    pub fn set_status_heartbeat_interval(&mut self, seconds: i32) -> bool {
        match self.dedicated.management_server.as_ref() {
            Some(management) => {
                let rescheduled = management
                    .schedule_heartbeat(self.notification_manager.clone(), i64::from(seconds));
                if rescheduled {
                    self.dedicated
                        .settings
                        .update(|p| p.update(&p.status_heartbeat_interval, seconds));
                }
                rescheduled
            }
            None => false,
        }
    }

    fn get_management_setting(&mut self, key: SettingKey) -> SettingValue {
        let p = self.properties();
        match key {
            SettingKey::AutoSave => SettingValue::Bool(self.is_auto_save()),
            SettingKey::Difficulty => SettingValue::Difficulty(*p.difficulty.get()),
            SettingKey::EnforceWhitelist => SettingValue::Bool(*p.enforce_whitelist.get()),
            SettingKey::UsingWhitelist => SettingValue::Bool(*p.white_list.get()),
            SettingKey::MaxPlayers => SettingValue::Int(*p.max_players.get()),
            SettingKey::PauseWhenEmptySeconds => {
                SettingValue::Int(*p.pause_when_empty_seconds.get())
            }
            SettingKey::PlayerIdleTimeout => SettingValue::Int(*p.player_idle_timeout.get()),
            SettingKey::AllowFlight => SettingValue::Bool(*p.allow_flight.get()),
            SettingKey::SpawnProtectionRadius => SettingValue::Int(*p.spawn_protection.get()),
            SettingKey::Motd => SettingValue::Str(p.motd.get().clone()),
            SettingKey::ForceGameMode => SettingValue::Bool(*p.force_game_mode.get()),
            SettingKey::GameMode => SettingValue::GameType(*p.game_mode.get()),
            SettingKey::ViewDistance => SettingValue::Int(*p.view_distance.get()),
            SettingKey::SimulationDistance => SettingValue::Int(*p.simulation_distance.get()),
            SettingKey::AcceptsTransfers => SettingValue::Bool(*p.accepts_transfers.get()),
            SettingKey::StatusHeartbeatInterval => {
                SettingValue::Int(*p.status_heartbeat_interval.get())
            }
            SettingKey::OperatorUserPermissions => {
                SettingValue::PermissionLevel(p.op_permissions.get().level())
            }
            SettingKey::HidesOnlinePlayers => SettingValue::Bool(*p.hide_online_players.get()),
            SettingKey::RepliesToStatus => SettingValue::Bool(*p.enable_status.get()),
            SettingKey::EntityBroadcastRangePercentage => {
                SettingValue::Int(*p.entity_broadcast_range_percentage.get())
            }
        }
    }

    fn set_management_setting(&mut self, key: SettingKey, value: SettingValue) {
        match (key, value) {
            (SettingKey::AutoSave, SettingValue::Bool(b)) => {
                self.set_auto_save(b);
            }
            (SettingKey::Difficulty, SettingValue::Difficulty(d)) => {
                self.set_difficulty_property(d)
            }
            (SettingKey::EnforceWhitelist, SettingValue::Bool(b)) => self.set_enforce_whitelist(b),
            (SettingKey::UsingWhitelist, SettingValue::Bool(b)) => {
                self.set_using_whitelist(b);
                self.player_list.settings.using_whitelist = b;
            }
            (SettingKey::MaxPlayers, SettingValue::Int(i)) => {
                self.set_max_players(i);
                self.player_list.settings.max_players = i;
            }
            (SettingKey::PauseWhenEmptySeconds, SettingValue::Int(i)) => {
                self.set_pause_when_empty_seconds(i)
            }
            (SettingKey::PlayerIdleTimeout, SettingValue::Int(i)) => {
                self.set_player_idle_timeout(i)
            }
            (SettingKey::AllowFlight, SettingValue::Bool(b)) => self.set_allow_flight(b),
            (SettingKey::SpawnProtectionRadius, SettingValue::Int(i)) => {
                self.set_spawn_protection_radius(i)
            }
            (SettingKey::Motd, SettingValue::Str(s)) => self.set_motd(&s),
            (SettingKey::ForceGameMode, SettingValue::Bool(b)) => {
                self.dedicated
                    .settings
                    .update(|p| p.update(&p.force_game_mode, b));
            }
            (SettingKey::GameMode, SettingValue::GameType(g)) => self.set_game_mode_property(g),
            (SettingKey::ViewDistance, SettingValue::Int(i)) => self.set_view_distance(i),
            (SettingKey::SimulationDistance, SettingValue::Int(i)) => {
                self.set_simulation_distance(i)
            }
            (SettingKey::AcceptsTransfers, SettingValue::Bool(b)) => self.set_accepts_transfers(b),
            (SettingKey::OperatorUserPermissions, SettingValue::PermissionLevel(l)) => {
                let permissions = LevelBasedPermissionSet::for_level(l);
                self.set_operator_user_permissions(permissions);
                self.player_list.settings.operator_user_permissions = permissions;
                self.player_list.refresh_player_permissions();
            }
            (SettingKey::HidesOnlinePlayers, SettingValue::Bool(b)) => {
                self.set_hides_online_players(b)
            }
            (SettingKey::RepliesToStatus, SettingValue::Bool(b)) => self.set_replies_to_status(b),
            (SettingKey::EntityBroadcastRangePercentage, SettingValue::Int(i)) => {
                self.set_entity_broadcast_range_percentage(i)
            }
            (key, value) => {
                log::warn!("Ignoring management setting {key:?} with mismatched value {value:?}")
            }
        }
    }

    /// `DedicatedServer.setGameMode`: persists the property and changes the world's default game type.
    pub fn set_game_mode_property(&mut self, game_mode: GameType) {
        self.dedicated
            .settings
            .update(|p| p.update(&p.game_mode, game_mode));
        self.world_data.lock().settings.game_type = game_mode;
    }
}

/// `DedicatedServer.runCommand` for the rcon thread: run `command` as the rcon source on the game thread and return the
/// collected feedback.
pub struct ServerRconRunner {
    shared: Arc<ServerShared>,
    source: Arc<RconConsoleSource>,
}

impl ServerRconRunner {
    pub fn new(shared: Arc<ServerShared>, broadcast_to_ops: bool) -> Self {
        Self {
            shared,
            source: Arc::new(RconConsoleSource::new(Arc::new(AtomicBool::new(
                broadcast_to_ops,
            )))),
        }
    }
}

impl RconCommandRunner for ServerRconRunner {
    fn run(&self, cmd: &str) -> String {
        self.source.prepare_for_command();
        let (source, command) = (self.source.clone(), cmd.to_owned());
        // Same lifetime reasoning as `on_server`: the closure is `'static` already, no transmute needed.
        self.shared.execute_blocking(move |server| {
            let stack = server.create_rcon_command_source_stack(source);
            let commands = server.commands.clone();
            commands.perform_prefixed_command(server, stack, &command);
        });
        self.source.get_command_response()
    }
}

impl MinecraftServer {
    /// `rconConsoleSource.createCommandSourceStack()`: owner permissions, named `Rcon`, at the respawn position.
    pub fn create_rcon_command_source_stack(
        &self,
        source: Arc<RconConsoleSource>,
    ) -> crate::commands::CommandSourceStack {
        let respawn = self.get_respawn_data();
        crate::commands::CommandSourceStack::named(
            source,
            mc_core::phys::Vec3::new(
                f64::from(respawn.pos.x()),
                f64::from(respawn.pos.y()),
                f64::from(respawn.pos.z()),
            ),
            mc_core::phys::Vec2::ZERO,
            self.find_respawn_dimension(),
            LevelBasedPermissionSet::OWNER.into(),
            RconConsoleSource::name(),
        )
    }
}
