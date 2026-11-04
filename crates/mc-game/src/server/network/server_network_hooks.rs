//! `NetworkHooks` for the real server: reads what is mirrored in `ServerShared` (status, server info), the settings and
//! configuration snapshots the lifecycle installs, and forwards the world-side calls to the player list.

use super::config::PrepareSpawn;
use super::hooks::{ConfigurationData, GameThread, LegacyServerInfo, NetworkHooks, NetworkSettings, ServerHandle};
use crate::server::{MinecraftServer, ServerShared};
use mc_protocol::auth::{ServerKeyPair, SessionClient};
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::status::ServerStatus;
use mc_text::Component;
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

/// The game-thread half of `NetworkHooks`: the `PlayerList` / spawn operations (implemented next to the player list).
pub trait PlayerListHooks: Send + Sync {
    /// `PlayerList.canPlayerLogin(address, new NameAndId(profile))`.
    fn can_player_login(&self, server: &mut MinecraftServer, addr: SocketAddr, profile: &GameProfile) -> Option<Component>;
    /// `PlayerList.disconnectAllPlayersWithProfile(id)`.
    fn disconnect_all_players_with_profile(&self, server: &mut MinecraftServer, id: Uuid) -> bool;
    /// `PlayerList.getPlayer(id) != null`.
    fn is_player_in_world(&self, server: &mut MinecraftServer, id: Uuid) -> bool;
    /// `serverActivityMonitor.reportLoginActivity()`.
    fn report_login_activity(&self, server: &mut MinecraftServer);
    /// `new PrepareSpawnTask(server, nameAndId)`.
    fn prepare_spawn(&self, server: &mut MinecraftServer, profile: &GameProfile) -> Box<dyn PrepareSpawn>;
}

pub struct ServerNetworkHooks {
    shared: Arc<ServerShared>,
    settings: RwLock<NetworkSettings>,
    key_pair: Arc<ServerKeyPair>,
    session: SessionClient,
    configuration: RwLock<Arc<ConfigurationData>>,
    players: OnceLock<Arc<dyn PlayerListHooks>>,
}

impl ServerNetworkHooks {
    pub fn new(
        shared: Arc<ServerShared>,
        settings: NetworkSettings,
        key_pair: Arc<ServerKeyPair>,
        session: SessionClient,
        configuration: Arc<ConfigurationData>,
    ) -> Self {
        Self { shared, settings: RwLock::new(settings), key_pair, session, configuration: RwLock::new(configuration), players: OnceLock::new() }
    }

    /// Installs the player-list side once `PlayerList` exists (before the listener accepts connections).
    pub fn set_player_list_hooks(&self, hooks: Arc<dyn PlayerListHooks>) {
        let _ = self.players.set(hooks);
    }

    /// Runtime changes of server settings (management protocol: `accepts-transfers`, ...).
    pub fn update_settings(&self, f: impl FnOnce(&mut NetworkSettings)) {
        f(&mut self.settings.write());
    }

    /// New registries/tags/packs after `/reload`.
    pub fn set_configuration(&self, configuration: Arc<ConfigurationData>) {
        *self.configuration.write() = configuration;
    }

    fn players(&self) -> &Arc<dyn PlayerListHooks> {
        self.players.get().expect("player list hooks are installed before the listener accepts connections")
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
        LegacyServerInfo { motd: info.motd, player_count: info.player_count, max_players: info.max_players, server_version: info.server_version }
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
    fn can_player_login(&self, server: &mut dyn ServerHandle, addr: SocketAddr, profile: &GameProfile) -> Option<Component> {
        self.players().can_player_login(real(server), addr, profile)
    }
    fn disconnect_all_players_with_profile(&self, server: &mut dyn ServerHandle, id: Uuid) -> bool {
        self.players().disconnect_all_players_with_profile(real(server), id)
    }
    fn is_player_in_world(&self, server: &mut dyn ServerHandle, id: Uuid) -> bool {
        self.players().is_player_in_world(real(server), id)
    }
    fn report_login_activity(&self, server: &mut dyn ServerHandle) {
        self.players().report_login_activity(real(server));
    }
    fn prepare_spawn(&self, server: &mut dyn ServerHandle, profile: &GameProfile) -> Box<dyn PrepareSpawn> {
        self.players().prepare_spawn(real(server), profile)
    }
}

fn real(server: &mut dyn ServerHandle) -> &mut MinecraftServer {
    server.minecraft_server().expect("the real network hooks run against the real server")
}
