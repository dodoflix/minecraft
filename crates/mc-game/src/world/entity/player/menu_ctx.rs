//! `MenuPlayer` over a `(ServerPlayer, ServerLevel)` pair: what menus/slots/inventory algorithms ask of the player.

use super::ServerPlayer;
use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::player::inventory::Inventory;
use crate::world::inventory::{ConnectionContainerSynchronizer, ContainerListener, ContainerSynchronizer, MenuInit, MenuPlayer, Prediction, Slot};
use crate::world::item::ItemStack;
use mc_core::pos::BlockPos;
use mc_protocol::packets::play::ClientboundSetPlayerInventoryPacket;

pub struct PlayerCtx<'a> {
    pub player: &'a mut ServerPlayer,
    pub level: &'a mut ServerLevel,
}

impl MenuPlayer for PlayerCtx<'_> {
    fn entity_id(&self) -> i32 {
        self.player.base_id()
    }
    fn inventory(&self) -> &Inventory {
        &self.player.player.inventory
    }
    fn inventory_mut(&mut self) -> &mut Inventory {
        &mut self.player.player.inventory
    }
    fn has_infinite_materials(&self) -> bool {
        self.player.player.abilities.instabuild
    }
    fn is_creative(&self) -> bool {
        self.player.is_creative()
    }
    fn is_spectator(&self) -> bool {
        use crate::world::entity::Entity;
        self.player.is_spectator()
    }
    fn is_removed_not_changing_dimension(&self) -> bool {
        use crate::world::entity::{Entity, RemovalReason};
        self.player.base().removal_reason().is_some_and(|r| r != RemovalReason::ChangedDimension)
    }
    fn has_disconnected(&self) -> bool {
        self.player.disconnected
    }
    /// `Player.drop(...)`: the item-entity class does not exist yet, so the stack is queued in
    /// `ServerPlayer::pending_drops` for the item-entity slice to spawn (vanilla would `addFreshEntity` it now).
    fn drop(&mut self, stack: ItemStack, thrown_from_hand: bool, _prediction: Prediction) {
        if !stack.is_empty() {
            self.player.pending_drops.push((stack, thrown_from_hand));
        }
    }
    fn on_equip_item(&mut self, _slot: EquipmentSlot, _old: &ItemStack, _new: &ItemStack) {
        // equip sound + `EQUIP` game event are dispatched by the sound/game-event layer
    }
    fn is_within_block_interaction_range(&self, pos: BlockPos, buffer: f64) -> bool {
        self.player.is_within_block_interaction_range(pos, buffer)
    }
    fn send_inventory_slot_update(&mut self, slot: usize, stack: ItemStack) {
        self.player.send(&ClientboundSetPlayerInventoryPacket { slot: slot as i32, contents: stack });
    }
    fn level(&mut self) -> Option<&mut ServerLevel> {
        Some(self.level)
    }
}

/// `ServerPlayer.containerSynchronizer` + `containerListener`.
pub struct ServerPlayerMenuInit {
    pub connection: crate::server::network::ConnectionSender,
}

struct NoopListener;
impl ContainerListener for NoopListener {
    /// vanilla triggers `INVENTORY_CHANGED` advancement criteria here
    fn slot_changed(&mut self, _slot: &Slot, _slot_index: usize, _stack: &ItemStack) {}
}

impl MenuInit for ServerPlayerMenuInit {
    fn container_synchronizer(&self) -> Box<dyn ContainerSynchronizer> {
        Box::new(ConnectionContainerSynchronizer::new(self.connection.clone()))
    }
    fn container_listener(&self) -> Box<dyn ContainerListener> {
        Box::new(NoopListener)
    }
}
