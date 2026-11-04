//! Shared harness for the network integration tests: a mock server behind `NetworkHooks`, a game-thread loop, and
//! client helpers built on `mc_protocol::net`.
#![allow(dead_code)]

use indexmap::IndexMap;
use mc_core::ident::Identifier;
use mc_game::server::network::config::PrepareSpawn;
use mc_game::server::network::configuration_task::TaskError;
use mc_game::server::network::PendingPlayer;
use mc_game::server::network::hooks::{ConfigurationData, LegacyServerInfo, RegistrySynchronization};
use mc_game::server::network::{NetworkHooks, NetworkSettings, ServerConnectionListener};
use mc_game::server::{MinecraftServer, ServerShared};
use mc_protocol::auth::{ServerKeyPair, SessionClient};
use mc_protocol::codec::GameProfile;
use mc_protocol::net::Connection;
use mc_protocol::packets::common::UntrustedEntry;
use mc_protocol::packets::configuration::{ClientboundRegistryDataPacket, ClientboundUpdateTagsPacket, KnownPack};
use mc_protocol::packets::handshake::{ClientIntent, ClientIntentionPacket, ServerboundHandshakePacket};
use mc_protocol::packets::status::{Players, ServerStatus, Version};
use mc_text::Component;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use uuid::Uuid;

pub struct NoRegistries;
impl RegistrySynchronization for NoRegistries {
    fn pack_registries(&self, _negotiated: &HashSet<KnownPack>) -> Vec<ClientboundRegistryDataPacket> {
        Vec::new()
    }
    fn tags(&self) -> ClientboundUpdateTagsPacket {
        ClientboundUpdateTagsPacket { tags: Vec::new() }
    }
}

pub struct MockHooks {
    pub settings: Mutex<NetworkSettings>,
    pub status: Mutex<Option<Arc<ServerStatus>>>,
    pub key_pair: Arc<ServerKeyPair>,
    pub session: Mutex<SessionClient>,
    /// What `can_player_login` answers.
    pub login_error: Mutex<Option<Component>>,
    /// Profiles `is_player_in_world` reports as online.
    pub online: Mutex<HashSet<Uuid>>,
    pub configuration: Mutex<Arc<ConfigurationData>>,
    pub log: Mutex<Vec<String>>,
    /// Players handed to the (mock) play side.
    pub spawned: Arc<Mutex<Vec<PendingPlayer>>>,
}

/// A `PrepareSpawn` that is ready at once and records the spawned player.
pub struct MockSpawn {
    pub spawned: Arc<Mutex<Vec<PendingPlayer>>>,
}

impl PrepareSpawn for MockSpawn {
    fn start(&mut self, _server: &mut MinecraftServer) -> Result<(), TaskError> {
        Ok(())
    }
    fn tick(&mut self, _server: &mut MinecraftServer) -> Result<bool, TaskError> {
        Ok(true)
    }
    fn keep_alive(&mut self, _server: &mut MinecraftServer) {}
    fn close(&mut self, _server: &mut MinecraftServer) {}
    fn spawn_player(&mut self, _server: &mut MinecraftServer, pending: PendingPlayer) -> Result<(), TaskError> {
        self.spawned.lock().push(pending);
        Ok(())
    }
}

impl MockHooks {
    pub fn new() -> Arc<Self> {
        let status = ServerStatus {
            description: Component::literal("A Minecraft Server"),
            players: Some(Players { max: 20, online: 0, sample: Vec::new() }),
            version: Some(Version { name: "26.3".into(), protocol: mc_protocol::PROTOCOL_VERSION }),
            favicon: None,
            enforces_secure_chat: false,
        };
        Arc::new(Self {
            settings: Mutex::new(NetworkSettings { uses_authentication: false, ..NetworkSettings::default() }),
            status: Mutex::new(Some(Arc::new(status))),
            key_pair: Arc::new(ServerKeyPair::generate().unwrap()),
            session: Mutex::new(SessionClient::default()),
            login_error: Mutex::new(None),
            online: Mutex::new(HashSet::new()),
            configuration: Mutex::new(Arc::new(default_configuration())),
            log: Mutex::new(Vec::new()),
            spawned: Arc::new(Mutex::new(Vec::new())),
        })
    }
}

pub fn default_configuration() -> ConfigurationData {
    ConfigurationData {
        known_packs: vec![KnownPack { namespace: "minecraft".into(), id: "core".into(), version: "26.3".into() }],
        enabled_features: vec![Identifier::with_default_namespace("vanilla").unwrap()],
        server_links: Vec::<UntrustedEntry>::new(),
        code_of_conducts: IndexMap::new(),
        resource_pack: None,
        registries: Arc::new(NoRegistries),
    }
}

impl NetworkHooks for MockHooks {
    fn settings(&self) -> NetworkSettings {
        self.settings.lock().clone()
    }
    fn status(&self) -> Option<Arc<ServerStatus>> {
        self.status.lock().clone()
    }
    fn legacy_info(&self) -> LegacyServerInfo {
        LegacyServerInfo { motd: "A Minecraft Server".into(), player_count: 0, max_players: 20, server_version: "26.3".into() }
    }
    fn key_pair(&self) -> Arc<ServerKeyPair> {
        self.key_pair.clone()
    }
    fn session_client(&self) -> SessionClient {
        self.session.lock().clone()
    }
    fn configuration(&self) -> Arc<ConfigurationData> {
        self.configuration.lock().clone()
    }
    fn can_player_login(&self, _server: &mut MinecraftServer, _addr: SocketAddr, profile: &GameProfile) -> Option<Component> {
        self.log.lock().push(format!("can_player_login {}", profile.name));
        self.login_error.lock().clone()
    }
    fn disconnect_all_players_with_profile(&self, _server: &mut MinecraftServer, id: Uuid) -> bool {
        self.online.lock().contains(&id)
    }
    fn is_player_in_world(&self, _server: &mut MinecraftServer, id: Uuid) -> bool {
        self.online.lock().contains(&id)
    }
    fn report_login_activity(&self, _server: &mut MinecraftServer) {
        self.log.lock().push("login_activity".into());
    }
    fn prepare_spawn(&self, _server: &mut MinecraftServer, _profile: &GameProfile) -> Box<dyn PrepareSpawn> {
        Box::new(MockSpawn { spawned: self.spawned.clone() })
    }
}

/// A listening server on an ephemeral port with a game thread draining its task queue.
pub struct TestServer {
    pub hooks: Arc<MockHooks>,
    pub shared: Arc<ServerShared>,
    pub addr: SocketAddr,
    pub listener: ServerConnectionListener,
    game_thread: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    pub fn start(hooks: Arc<MockHooks>) -> Self {
        let (shared, rx) = ServerShared::new();
        let game_shared = shared.clone();
        let game_thread = std::thread::Builder::new()
            .name("Server thread".into())
            .spawn(move || {
                let mut server = MinecraftServer::new(game_shared.clone(), rx);
                while game_shared.is_running() {
                    server.run_all_tasks();
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
            .unwrap();
        let mut listener = ServerConnectionListener::new(hooks.clone(), shared.clone()).unwrap();
        let addr = listener.start_tcp_server_listener(Some("127.0.0.1".parse().unwrap()), 0).unwrap();
        Self { hooks, shared, addr, listener, game_thread: Some(game_thread) }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.listener.stop();
        self.shared.halt();
        if let Some(t) = self.game_thread.take() {
            let _ = t.join();
        }
    }
}

pub async fn connect(addr: SocketAddr) -> Connection<TcpStream> {
    Connection::new(TcpStream::connect(addr).await.unwrap())
}

pub async fn write_payload(conn: &mut Connection<TcpStream>, payload: &[u8]) {
    conn.write_payload(payload).await.unwrap();
    conn.flush().await.unwrap();
}

pub async fn handshake(conn: &mut Connection<TcpStream>, protocol_version: i32, intention: ClientIntent) {
    let mut buf = Vec::new();
    ServerboundHandshakePacket::ClientIntention(ClientIntentionPacket {
        protocol_version,
        host_name: "localhost".into(),
        port: 25565,
        intention,
    })
    .encode_payload(&mut buf);
    write_payload(conn, &buf).await;
}

pub async fn read(conn: &mut Connection<TcpStream>) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(10), conn.read_payload()).await.expect("read timed out").expect("read")
}

/// The next read must fail because the server closed the connection.
pub async fn expect_closed(conn: &mut Connection<TcpStream>) {
    match tokio::time::timeout(Duration::from_secs(10), conn.read_payload()).await {
        Ok(Err(_)) => {}
        Ok(Ok(p)) => panic!("expected the connection to be closed, got packet {p:?}"),
        Err(_) => panic!("connection not closed"),
    }
}
