use crate::server::MinecraftServer;
use crate::server::network::configuration_task::{ConfigurationTask, PacketSink, TaskError, TaskType};
use crate::server::network::hooks::RegistrySynchronization;
use mc_protocol::packets::configuration::{
    ClientboundConfigurationPacket, ClientboundSelectKnownPacks, KnownPack,
};
use std::collections::HashSet;
use std::sync::Arc;

/// `SynchronizeRegistriesTask`: offers the known packs, then sends registry data + tags according to the client's
/// answer. Cheap to clone (the registries are shared) because the listener keeps a handle for `handleSelectKnownPacks`.
#[derive(Clone)]
pub struct SynchronizeRegistriesTask {
    requested_packs: Vec<KnownPack>,
    registries: Arc<dyn RegistrySynchronization>,
}

impl SynchronizeRegistriesTask {
    pub const TYPE: TaskType = TaskType("synchronize_registries");

    pub fn new(known_packs: Vec<KnownPack>, registries: Arc<dyn RegistrySynchronization>) -> Self {
        Self { requested_packs: known_packs, registries }
    }

    fn send_registries(&self, connection: PacketSink, negotiated_packs: &HashSet<KnownPack>) {
        for packet in self.registries.pack_registries(negotiated_packs) {
            connection(ClientboundConfigurationPacket::RegistryData(packet));
        }
        connection(ClientboundConfigurationPacket::UpdateTags(self.registries.tags()));
    }

    /// `handleResponse`: a pack list identical to the requested one enables the known-pack optimisation.
    pub fn handle_response(&self, accepted_packs: &[KnownPack], connection: PacketSink) {
        if accepted_packs == self.requested_packs.as_slice() {
            self.send_registries(connection, &self.requested_packs.iter().cloned().collect());
        } else {
            self.send_registries(connection, &HashSet::new());
        }
    }
}

impl ConfigurationTask for SynchronizeRegistriesTask {
    fn start(&mut self, _server: &mut MinecraftServer, connection: PacketSink) -> Result<(), TaskError> {
        connection(ClientboundConfigurationPacket::SelectKnownPacks(ClientboundSelectKnownPacks { known_packs: self.requested_packs.clone() }));
        Ok(())
    }

    fn task_type(&self) -> TaskType {
        Self::TYPE
    }
}
