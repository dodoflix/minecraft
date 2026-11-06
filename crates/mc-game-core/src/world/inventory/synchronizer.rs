//! `ServerPlayer.containerSynchronizer`: sends menu state over the player's connection.

use super::{ContainerSynchronizer, RemoteSlot};
use crate::server::network::ConnectionSender;
use crate::world::item::ItemStack;
use mc_protocol::packets::play::{
    ClientboundContainerSetContentPacket, ClientboundContainerSetDataPacket,
    ClientboundContainerSetSlotPacket, ClientboundSetCursorItemPacket,
};

pub struct ConnectionContainerSynchronizer {
    connection: ConnectionSender,
}

impl ConnectionContainerSynchronizer {
    pub fn new(connection: ConnectionSender) -> Self {
        Self { connection }
    }
}

impl ContainerSynchronizer for ConnectionContainerSynchronizer {
    fn send_initial_data(
        &mut self,
        container_id: i32,
        state_id: i32,
        slot_items: Vec<ItemStack>,
        carried: ItemStack,
        data_slots: &[i32],
    ) {
        self.connection.send(&ClientboundContainerSetContentPacket {
            container_id,
            state_id,
            items: slot_items,
            carried_item: carried,
        });
        for (slot, value) in data_slots.iter().enumerate() {
            self.send_data_change(container_id, slot, *value);
        }
    }

    fn send_slot_change(
        &mut self,
        container_id: i32,
        state_id: i32,
        slot_index: usize,
        stack: ItemStack,
    ) {
        self.connection.send(&ClientboundContainerSetSlotPacket {
            container_id,
            state_id,
            slot: slot_index as i16,
            item_stack: stack,
        });
    }

    fn send_carried_change(&mut self, stack: ItemStack) {
        self.connection
            .send(&ClientboundSetCursorItemPacket { contents: stack });
    }

    fn send_data_change(&mut self, container_id: i32, id: usize, value: i32) {
        self.connection.send(&ClientboundContainerSetDataPacket {
            container_id,
            id: id as i16,
            value: value as i16,
        });
    }

    fn create_slot(&self) -> RemoteSlot {
        RemoteSlot::new_synchronized()
    }
}
