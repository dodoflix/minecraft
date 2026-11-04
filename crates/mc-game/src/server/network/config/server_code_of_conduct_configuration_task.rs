use crate::server::network::hooks::ServerHandle;
use crate::server::network::configuration_task::{ConfigurationTask, PacketSink, TaskError, TaskType};
use mc_protocol::packets::configuration::{ClientboundCodeOfConductPacket, ClientboundConfigurationPacket};

/// `ServerCodeOfConductConfigurationTask`: sends the code of conduct for the client's language; ends on
/// `ServerboundAcceptCodeOfConductPacket`.
pub struct ServerCodeOfConductConfigurationTask {
    code_of_conduct: Box<dyn Fn() -> String + Send>,
}

impl ServerCodeOfConductConfigurationTask {
    pub const TYPE: TaskType = TaskType("server_code_of_conduct");

    pub fn new(code_of_conduct: impl Fn() -> String + Send + 'static) -> Self {
        Self { code_of_conduct: Box::new(code_of_conduct) }
    }
}

impl ConfigurationTask for ServerCodeOfConductConfigurationTask {
    fn start(&mut self, _server: &mut dyn ServerHandle, connection: PacketSink) -> Result<(), TaskError> {
        connection(ClientboundConfigurationPacket::CodeOfConduct(ClientboundCodeOfConductPacket { code_of_conduct: (self.code_of_conduct)() }));
        Ok(())
    }

    fn task_type(&self) -> TaskType {
        Self::TYPE
    }
}
