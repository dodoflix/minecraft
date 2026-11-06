//! `ConfigurationTask`: one step of the configuration phase, run in order by `ServerConfigurationPacketListenerImpl`.

use super::hooks::ServerHandle;
use mc_protocol::packets::configuration::ClientboundConfigurationPacket;
use std::fmt;

/// `ConfigurationTask.Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskType(pub &'static str);

impl TaskType {
    pub fn id(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A failed task start/tick (a Java exception in vanilla): logged, then the client is disconnected with
/// `multiplayer.disconnect.configuration_error`.
pub type TaskError = Box<dyn std::error::Error + Send + Sync>;

/// The `Consumer<Packet<?>>` tasks send packets through.
pub type PacketSink<'a> = &'a mut dyn FnMut(ClientboundConfigurationPacket);

pub trait ConfigurationTask: Send {
    fn start(
        &mut self,
        server: &mut dyn ServerHandle,
        connection: PacketSink,
    ) -> Result<(), TaskError>;

    /// `true` once the task is finished (the default never finishes by itself: a client packet ends the task).
    fn tick(&mut self, _server: &mut dyn ServerHandle) -> Result<bool, TaskError> {
        Ok(false)
    }

    fn task_type(&self) -> TaskType;
}
