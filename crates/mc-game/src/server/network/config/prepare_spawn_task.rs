use crate::server::MinecraftServer;
use crate::server::network::configuration_task::{ConfigurationTask, PacketSink, TaskError, TaskType};
use crate::server::network::pending_player::PendingPlayer;
use parking_lot::Mutex;
use std::sync::Arc;

/// The world side of `PrepareSpawnTask` (player data, `PlayerSpawnFinder`, the spawn-chunk ticket, creating and
/// placing the `ServerPlayer`), implemented next to the player list and chunk map. All methods run on the game thread.
pub trait PrepareSpawn: Send {
    /// `start`: load the player data, pick the spawn level, start finding the spawn position.
    fn start(&mut self, server: &mut MinecraftServer) -> Result<(), TaskError>;
    /// `tick`: `true` once the spawn position is found and the chunks around it are loaded.
    fn tick(&mut self, server: &mut MinecraftServer) -> Result<bool, TaskError>;
    /// `keepAlive` (called every tick while the task state is `Ready`).
    fn keep_alive(&mut self, server: &mut MinecraftServer);
    /// `close` (the connection ended before the player was spawned).
    fn close(&mut self, server: &mut MinecraftServer);
    /// `spawnPlayer(connection, cookie)`: create the player and `PlayerList.placeNewPlayer`. An error is logged as
    /// `Couldn't place player in world` and answered with `multiplayer.disconnect.invalid_player_data`.
    fn spawn_player(&mut self, server: &mut MinecraftServer, pending: PendingPlayer) -> Result<(), TaskError>;
}

/// `PrepareSpawnTask`, as a queued configuration task that the listener also keeps a handle to.
#[derive(Clone)]
pub struct PrepareSpawnTask {
    inner: Arc<Mutex<Option<Box<dyn PrepareSpawn>>>>,
}

impl PrepareSpawnTask {
    pub const TYPE: TaskType = TaskType("prepare_spawn");
    /// `PREPARE_CHUNK_RADIUS`.
    pub const PREPARE_CHUNK_RADIUS: i32 = 3;

    pub fn new(spawn: Box<dyn PrepareSpawn>) -> Self {
        Self { inner: Arc::new(Mutex::new(Some(spawn))) }
    }

    pub fn spawn_player(&self, server: &mut MinecraftServer, pending: PendingPlayer) -> Result<(), TaskError> {
        match self.inner.lock().as_mut() {
            Some(spawn) => spawn.spawn_player(server, pending),
            None => Err("Player spawn was not ready".into()),
        }
    }

    pub fn keep_alive(&self, server: &mut MinecraftServer) {
        if let Some(spawn) = self.inner.lock().as_mut() {
            spawn.keep_alive(server);
        }
    }

    pub fn close(&self, server: &mut MinecraftServer) {
        if let Some(mut spawn) = self.inner.lock().take() {
            spawn.close(server);
        }
    }
}

impl ConfigurationTask for PrepareSpawnTask {
    fn start(&mut self, server: &mut MinecraftServer, _connection: PacketSink) -> Result<(), TaskError> {
        match self.inner.lock().as_mut() {
            Some(spawn) => spawn.start(server),
            None => Ok(()),
        }
    }

    fn tick(&mut self, server: &mut MinecraftServer) -> Result<bool, TaskError> {
        match self.inner.lock().as_mut() {
            Some(spawn) => spawn.tick(server),
            None => Ok(false),
        }
    }

    fn task_type(&self) -> TaskType {
        Self::TYPE
    }
}
