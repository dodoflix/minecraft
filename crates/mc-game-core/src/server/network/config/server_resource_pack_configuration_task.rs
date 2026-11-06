use crate::server::network::configuration_task::{
    ConfigurationTask, PacketSink, TaskError, TaskType,
};
use crate::server::network::hooks::ServerHandle;
use crate::server::network::hooks::ServerResourcePackInfo;
use mc_protocol::packets::configuration::{
    ClientboundConfigurationPacket, ClientboundResourcePackPushPacket,
};

/// `ServerResourcePackConfigurationTask`: pushes the server resource pack; ends when the client reports a final
/// state (`ServerboundResourcePackPacket` with a terminal action).
pub struct ServerResourcePackConfigurationTask {
    info: ServerResourcePackInfo,
}

impl ServerResourcePackConfigurationTask {
    pub const TYPE: TaskType = TaskType("server_resource_pack");

    pub fn new(info: ServerResourcePackInfo) -> Self {
        Self { info }
    }
}

impl ConfigurationTask for ServerResourcePackConfigurationTask {
    fn start(
        &mut self,
        _server: &mut dyn ServerHandle,
        connection: PacketSink,
    ) -> Result<(), TaskError> {
        connection(ClientboundConfigurationPacket::ResourcePackPush(
            ClientboundResourcePackPushPacket {
                id: self.info.id,
                url: self.info.url.clone(),
                hash: self.info.hash.clone(),
                required: self.info.is_required,
                prompt: self.info.prompt.clone(),
            },
        ));
        Ok(())
    }

    fn task_type(&self) -> TaskType {
        Self::TYPE
    }
}
