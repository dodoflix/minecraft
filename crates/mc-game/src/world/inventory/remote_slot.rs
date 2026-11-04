//! `RemoteSlot`, `ContainerSynchronizer`, `ContainerListener`.
use super::Slot;
use crate::world::item::{HashedStack, ItemStack};
use mc_protocol::components::hashed::component_hash;

/// What the client is believed to hold for one slot (or the cursor).
#[derive(Clone, Debug)]
pub enum RemoteSlot {
    /// `RemoteSlot.PLACEHOLDER` (no synchronizer attached): always matches.
    Placeholder,
    /// `RemoteSlot.Synchronized`.
    Synchronized { remote_stack: Option<ItemStack>, remote_hash: Option<HashedStack> },
}

impl RemoteSlot {
    pub fn new_synchronized() -> Self {
        Self::Synchronized { remote_stack: None, remote_hash: None }
    }

    pub fn force(&mut self, outgoing: &ItemStack) {
        if let Self::Synchronized { remote_stack, remote_hash } = self {
            *remote_stack = Some(outgoing.copy());
            *remote_hash = None;
        }
    }

    pub fn receive(&mut self, incoming: HashedStack) {
        if let Self::Synchronized { remote_stack, remote_hash } = self {
            *remote_stack = None;
            *remote_hash = Some(incoming);
        }
    }

    pub fn matches(&mut self, local: &ItemStack) -> bool {
        match self {
            Self::Placeholder => true,
            Self::Synchronized { remote_stack, remote_hash } => {
                if let Some(r) = remote_stack {
                    ItemStack::matches(r, local)
                } else if remote_hash.as_ref().is_some_and(|h| h.matches(local, &component_hash)) {
                    *remote_stack = Some(local.copy());
                    true
                } else {
                    false
                }
            }
        }
    }

    /// `Synchronized.copyFrom` (only between two synchronized slots).
    pub fn copy_from(&mut self, other: &RemoteSlot) {
        if let (Self::Synchronized { remote_stack, remote_hash }, Self::Synchronized { remote_stack: os, remote_hash: oh }) = (self, other) {
            *remote_stack = os.clone();
            *remote_hash = oh.clone();
        }
    }
}

/// `ContainerSynchronizer`: sends menu state to the client. `state_id` arguments are already incremented
/// (`container.incrementStateId()` in the vanilla implementation).
pub trait ContainerSynchronizer {
    fn send_initial_data(&mut self, container_id: i32, state_id: i32, slot_items: Vec<ItemStack>, carried: ItemStack, data_slots: &[i32]);
    fn send_slot_change(&mut self, container_id: i32, state_id: i32, slot_index: usize, stack: ItemStack);
    fn send_carried_change(&mut self, stack: ItemStack);
    fn send_data_change(&mut self, container_id: i32, id: usize, value: i32);
    fn create_slot(&self) -> RemoteSlot;
}

/// `ContainerListener`.
pub trait ContainerListener {
    fn slot_changed(&mut self, slot: &Slot, slot_index: usize, stack: &ItemStack);
    fn data_changed(&mut self, _id: usize, _value: i32) {}
}
