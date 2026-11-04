//! What the network layer needs from the rest of the server. The tokio connection tasks read settings/snapshots
//! through `&self` methods (callable from any thread) and run everything that needs the world on the game thread
//! through the methods taking `&mut MinecraftServer` (`on_server` hops there and back). The real implementation is
//! `ServerNetworkHooks`; tests use a mock.

use crate::server::MinecraftServer;
use crate::server::ServerShared;
use crate::server::network::config::PrepareSpawn;
use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_protocol::auth::{ServerKeyPair, SessionClient};
use mc_protocol::codec::GameProfile;
use mc_protocol::packets::common::UntrustedEntry;
use mc_protocol::packets::configuration::{ClientboundRegistryDataPacket, ClientboundUpdateTagsPacket, KnownPack};
use mc_protocol::packets::status::ServerStatus;
use mc_text::Component;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use uuid::Uuid;

/// `SharedConstants.getCurrentVersion().protocolVersion()`.
pub const PROTOCOL_VERSION: i32 = mc_protocol::PROTOCOL_VERSION;
/// `SharedConstants.getCurrentVersion().name()`.
pub const VERSION_NAME: &str = "26.3";

/// Server settings the network layer reads per connection (they can change at runtime through the management
/// protocol, so they are fetched fresh, not captured at start).
#[derive(Debug, Clone)]
pub struct NetworkSettings {
    /// `MinecraftServer.usesAuthentication()` (`online-mode`).
    pub uses_authentication: bool,
    /// `getCompressionThreshold()` (`network-compression-threshold`; negative = off).
    pub compression_threshold: i32,
    /// `getRateLimitPacketsPerSecond()` (`rate-limit`; `<= 0` = off).
    pub rate_limit_packets_per_second: i32,
    /// `getPreventProxyConnections()`.
    pub prevent_proxy_connections: bool,
    /// `acceptsTransfers()`.
    pub accepts_transfers: bool,
    /// `logIPs()`.
    pub log_ips: bool,
    /// `repliesToStatus()` (`enable-status`).
    pub replies_to_status: bool,
    /// `getSingleplayerProfile()` (always `None` on a dedicated server).
    pub singleplayer_profile: Option<GameProfile>,
    /// `getServerModName()`.
    pub server_mod_name: String,
}

impl Default for NetworkSettings {
    /// The dedicated server's defaults (`online-mode=true`, threshold 256, no rate limit, transfers off).
    fn default() -> Self {
        Self {
            uses_authentication: true,
            compression_threshold: 256,
            rate_limit_packets_per_second: 0,
            prevent_proxy_connections: false,
            accepts_transfers: false,
            log_ips: true,
            replies_to_status: true,
            singleplayer_profile: None,
            server_mod_name: "vanilla".to_owned(),
        }
    }
}

/// `ServerInfo`, as the legacy (pre-1.7) ping reports it.
#[derive(Debug, Clone)]
pub struct LegacyServerInfo {
    pub motd: String,
    pub player_count: i32,
    pub max_players: i32,
    pub server_version: String,
}

/// `MinecraftServer.ServerResourcePackInfo` (`resource-pack*` properties).
#[derive(Debug, Clone, PartialEq)]
pub struct ServerResourcePackInfo {
    pub id: Uuid,
    pub url: String,
    pub hash: String,
    pub is_required: bool,
    pub prompt: Option<Component>,
}

/// `RegistrySynchronization.packRegistries` + `TagNetworkSerialization.serializeTagsToNetwork` of the current
/// registries (implemented by the registry layer).
pub trait RegistrySynchronization: Send + Sync {
    /// The `ClientboundRegistryDataPacket`s in vanilla's order; entries the client already knows from one of the
    /// `negotiated` packs are sent without data.
    fn pack_registries(&self, negotiated: &HashSet<KnownPack>) -> Vec<ClientboundRegistryDataPacket>;
    /// `ClientboundUpdateTagsPacket(serializeTagsToNetwork(registries))`.
    fn tags(&self) -> ClientboundUpdateTagsPacket;
}

/// Everything `ServerConfigurationPacketListenerImpl.startConfiguration` reads off the server, as a snapshot (replaced
/// on `/reload`, like vanilla re-reading the live objects per login).
pub struct ConfigurationData {
    /// `getResourceManager().listPacks()` → `knownPackInfo()`.
    pub known_packs: Vec<KnownPack>,
    /// `FeatureFlags.REGISTRY.toNames(worldData.enabledFeatures())`.
    pub enabled_features: Vec<Identifier>,
    /// `serverLinks().untrust()`; empty = no packet.
    pub server_links: Vec<UntrustedEntry>,
    /// `getCodeOfConducts()`: language (lowercase) → text, in iteration order.
    pub code_of_conducts: IndexMap<String, String>,
    /// `getServerResourcePack()`.
    pub resource_pack: Option<ServerResourcePackInfo>,
    pub registries: Arc<dyn RegistrySynchronization>,
}

impl ConfigurationData {
    /// `MinecraftServer.isResourcePackRequired`.
    pub fn is_resource_pack_required(&self) -> bool {
        self.resource_pack.as_ref().is_some_and(|p| p.is_required)
    }
}

/// See the module documentation.
pub trait NetworkHooks: Send + Sync + 'static {
    fn settings(&self) -> NetworkSettings;
    /// `MinecraftServer.getStatus()`; `None` until the server is ready (then status requests are ignored).
    fn status(&self) -> Option<Arc<ServerStatus>>;
    fn legacy_info(&self) -> LegacyServerInfo;
    /// `MinecraftServer.getKeyPair()`.
    fn key_pair(&self) -> Arc<ServerKeyPair>;
    /// `services().sessionService()`.
    fn session_client(&self) -> SessionClient;
    fn configuration(&self) -> Arc<ConfigurationData>;

    // ---- game thread -----------------------------------------------------------------------------------------

    /// `PlayerList.canPlayerLogin(address, new NameAndId(profile))`: the disconnect reason, if the login must be refused.
    fn can_player_login(&self, server: &mut MinecraftServer, addr: SocketAddr, profile: &GameProfile) -> Option<Component>;
    /// `PlayerList.disconnectAllPlayersWithProfile(id)`: `true` if a player had to be disconnected (wait for it).
    fn disconnect_all_players_with_profile(&self, server: &mut MinecraftServer, id: Uuid) -> bool;
    /// `PlayerList.getPlayer(id) != null`.
    fn is_player_in_world(&self, server: &mut MinecraftServer, id: Uuid) -> bool;
    /// `serverActivityMonitor.reportLoginActivity()`.
    fn report_login_activity(&self, server: &mut MinecraftServer);
    /// `new PrepareSpawnTask(server, nameAndId)`.
    fn prepare_spawn(&self, server: &mut MinecraftServer, profile: &GameProfile) -> Box<dyn PrepareSpawn>;
}

/// Runs `f` on the game thread (`MinecraftServer.execute`) and returns its result; `None` if the server is gone.
pub async fn on_server<R: Send + 'static>(shared: &ServerShared, f: impl FnOnce(&mut MinecraftServer) -> R + Send + 'static) -> Option<R> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    shared.execute(move |server| {
        let _ = tx.send(f(server));
    });
    rx.await.ok()
}
