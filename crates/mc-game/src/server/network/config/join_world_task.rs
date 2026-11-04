use crate::server::MinecraftServer;
use crate::server::network::configuration_task::{ConfigurationTask, PacketSink, TaskError, TaskType};
use mc_protocol::packets::configuration::{ClientboundConfigurationPacket, ClientboundFinishConfigurationPacket};

/// `JoinWorldTask`: sends `ClientboundFinishConfigurationPacket`; ends when the client answers.
pub struct JoinWorldTask;

impl JoinWorldTask {
    pub const TYPE: TaskType = TaskType("join_world");
}

impl ConfigurationTask for JoinWorldTask {
    fn start(&mut self, _server: &mut MinecraftServer, connection: PacketSink) -> Result<(), TaskError> {
        connection(ClientboundConfigurationPacket::FinishConfiguration(ClientboundFinishConfigurationPacket));
        Ok(())
    }

    fn task_type(&self) -> TaskType {
        Self::TYPE
    }
}
