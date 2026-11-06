//! `NetworkHooks` for the real server: reads what is mirrored in `ServerShared` (status, server info), the settings and
//! configuration snapshots the lifecycle installs, and forwards the world-side calls to the player list.

use super::config::PrepareSpawn;
use super::configuration_task::TaskError;
use super::hooks::{
    ConfigurationData, GameThread, LegacyServerInfo, NetworkHooks, NetworkSettings, ServerHandle,
};
use super::pending_player::PendingPlayer;
use crate::server::notifications::ServerActivityMonitor;
use crate::server::players::NameAndId;
use crate::server::{MinecraftServer, ServerShared};
use mc_protocol::auth::{ServerKeyPair, SessionClient};
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::status::ServerStatus;
use mc_text::Component;
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

/// Creates the world side of a `PrepareSpawnTask` (player data, spawn finder, chunk ticket, `placeNewPlayer`).
pub type PrepareSpawnFactory =
    Box<dyn Fn(&mut MinecraftServer, &GameProfile) -> Box<dyn PrepareSpawn> + Send + Sync>;

/// The game-thread half of the hooks: forwards to the `PlayerList` and the spawn preparation.
#[derive(Default)]
pub struct PlayerListHooks {
    activity: OnceLock<Arc<ServerActivityMonitor>>,
    prepare_spawn: OnceLock<PrepareSpawnFactory>,
}

impl PlayerListHooks {
    /// Installs `MinecraftServer.serverActivityMonitor` (`reportLoginActivity` target).
    pub fn set_activity_monitor(&self, monitor: Arc<ServerActivityMonitor>) {
        let _ = self.activity.set(monitor);
    }

    /// Installs the `PrepareSpawnTask` implementation. Until then a login ends in the configuration phase with
    /// `multiplayer.disconnect.configuration_error` (and an error in the log).
    pub fn set_prepare_spawn(&self, factory: PrepareSpawnFactory) {
        let _ = self.prepare_spawn.set(factory);
    }

    fn can_player_login(
        &self,
        server: &mut MinecraftServer,
        addr: SocketAddr,
        profile: &GameProfile,
    ) -> Option<Component> {
        server
            .player_list
            .can_player_login(&addr, &NameAndId::new(profile.id, profile.name.clone()))
    }

    fn disconnect_all_players_with_profile(&self, server: &mut MinecraftServer, id: Uuid) -> bool {
        server.player_list.disconnect_all_players_with_profile(&id)
    }

    fn is_player_in_world(&self, server: &mut MinecraftServer, id: Uuid) -> bool {
        server.player_list.get_player(&id).is_some()
    }

    fn report_login_activity(&self) {
        if let Some(monitor) = self.activity.get() {
            monitor.report_login_activity();
        }
    }

    fn prepare_spawn(
        &self,
        server: &mut MinecraftServer,
        profile: &GameProfile,
    ) -> Box<dyn PrepareSpawn> {
        match self.prepare_spawn.get() {
            Some(factory) => factory(server, profile),
            None => Box::new(UnavailableSpawn),
        }
    }
}

/// `PrepareSpawn` when no implementation is installed: the configuration task fails like a vanilla task that threw.
struct UnavailableSpawn;

impl PrepareSpawn for UnavailableSpawn {
    fn start(&mut self, _server: &mut dyn ServerHandle) -> Result<(), TaskError> {
        Err("spawning players is not available in this server build".into())
    }
    fn tick(&mut self, _server: &mut dyn ServerHandle) -> Result<bool, TaskError> {
        Ok(false)
    }
    fn keep_alive(&mut self, _server: &mut dyn ServerHandle) {}
    fn close(&mut self, _server: &mut dyn ServerHandle) {}
    fn spawn_player(
        &mut self,
        _server: &mut dyn ServerHandle,
        _pending: PendingPlayer,
    ) -> Result<(), TaskError> {
        Err("spawning players is not available in this server build".into())
    }
}

pub struct ServerNetworkHooks {
    shared: Arc<ServerShared>,
    settings: RwLock<NetworkSettings>,
    key_pair: Arc<ServerKeyPair>,
    session: SessionClient,
    configuration: RwLock<Arc<ConfigurationData>>,
    players: PlayerListHooks,
}

impl ServerNetworkHooks {
    pub fn new(
        shared: Arc<ServerShared>,
        settings: NetworkSettings,
        key_pair: Arc<ServerKeyPair>,
        session: SessionClient,
        configuration: Arc<ConfigurationData>,
    ) -> Self {
        Self {
            shared,
            settings: RwLock::new(settings),
            key_pair,
            session,
            configuration: RwLock::new(configuration),
            players: PlayerListHooks::default(),
        }
    }

    /// The world side (`PlayerList`, spawn preparation): install the activity monitor and the spawn task here.
    pub fn player_list_hooks(&self) -> &PlayerListHooks {
        &self.players
    }

    /// Runtime changes of server settings (management protocol: `accepts-transfers`, ...).
    pub fn update_settings(&self, f: impl FnOnce(&mut NetworkSettings)) {
        f(&mut self.settings.write());
    }

    /// New registries/tags/packs after `/reload`.
    pub fn set_configuration(&self, configuration: Arc<ConfigurationData>) {
        *self.configuration.write() = configuration;
    }
}

impl NetworkHooks for ServerNetworkHooks {
    fn settings(&self) -> NetworkSettings {
        self.settings.read().clone()
    }
    fn status(&self) -> Option<Arc<ServerStatus>> {
        self.shared.status()
    }
    fn legacy_info(&self) -> LegacyServerInfo {
        let info = self.shared.info();
        LegacyServerInfo {
            motd: info.motd,
            player_count: info.player_count,
            max_players: info.max_players,
            server_version: info.server_version,
        }
    }
    fn key_pair(&self) -> Arc<ServerKeyPair> {
        self.key_pair.clone()
    }
    fn session_client(&self) -> SessionClient {
        self.session.clone()
    }
    fn configuration(&self) -> Arc<ConfigurationData> {
        self.configuration.read().clone()
    }
    fn game_thread(&self) -> Arc<dyn GameThread> {
        self.shared.clone()
    }
    fn can_player_login(
        &self,
        server: &mut dyn ServerHandle,
        addr: SocketAddr,
        profile: &GameProfile,
    ) -> Option<Component> {
        self.players.can_player_login(real(server), addr, profile)
    }
    fn disconnect_all_players_with_profile(&self, server: &mut dyn ServerHandle, id: Uuid) -> bool {
        self.players
            .disconnect_all_players_with_profile(real(server), id)
    }
    fn is_player_in_world(&self, server: &mut dyn ServerHandle, id: Uuid) -> bool {
        self.players.is_player_in_world(real(server), id)
    }
    fn report_login_activity(&self, _server: &mut dyn ServerHandle) {
        self.players.report_login_activity();
    }
    fn prepare_spawn(
        &self,
        server: &mut dyn ServerHandle,
        profile: &GameProfile,
    ) -> Box<dyn PrepareSpawn> {
        self.players.prepare_spawn(real(server), profile)
    }
}

fn real(server: &mut dyn ServerHandle) -> &mut MinecraftServer {
    server
        .minecraft_server()
        .expect("the real network hooks run against the real server")
}
