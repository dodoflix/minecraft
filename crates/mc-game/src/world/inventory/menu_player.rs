//! `MenuPlayer`: the part of vanilla's `Player`/`ServerPlayer`/`LivingEntity` that menus, slots and the inventory
//! algorithms consult. Implemented by the real player context (level + player) and by test players.

use crate::server::level::ServerLevel;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::entity::player::inventory::Inventory;
use crate::world::item::ItemStack;
use mc_core::pos::BlockPos;

/// `net.minecraft.util.Prediction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prediction {
    Predicted,
    ServerOnly,
}

/// `Equippable.slot()` of `stack` (the `minecraft:equippable` component), if any.
fn equippable_slot(stack: &ItemStack) -> Option<EquipmentSlot> {
    stack.get_json("minecraft:equippable")?.get("slot")?.as_str().and_then(EquipmentSlot::by_name)
}

pub trait MenuPlayer {
    fn entity_id(&self) -> i32;
    fn inventory(&self) -> &Inventory;
    fn inventory_mut(&mut self) -> &mut Inventory;
    /// `hasInfiniteMaterials()` (`abilities.instabuild`).
    fn has_infinite_materials(&self) -> bool;
    fn is_creative(&self) -> bool;
    fn is_spectator(&self) -> bool;
    /// `player instanceof ServerPlayer`.
    fn is_server_player(&self) -> bool {
        true
    }
    /// `isRemoved() && getRemovalReason() != CHANGED_DIMENSION`.
    fn is_removed_not_changing_dimension(&self) -> bool {
        false
    }
    fn has_disconnected(&self) -> bool {
        false
    }
    /// `Player.canDropItems()`.
    fn can_drop_items(&self) -> bool {
        true
    }
    /// `Player.handleCreativeModeItemDrop` (no-op for plain players).
    fn handle_creative_mode_item_drop(&mut self, _stack: &ItemStack) {}
    /// `Player.drop(stack, thrownFromHand, prediction)`: spawns the item entity (swing, `createItemStackToDrop`).
    fn drop(&mut self, stack: ItemStack, thrown_from_hand: bool, prediction: Prediction);
    /// `LivingEntity.onEquipItem(slot, old, new)` (equip sound + game event).
    fn on_equip_item(&mut self, slot: EquipmentSlot, old: &ItemStack, new: &ItemStack);
    /// `LivingEntity.canUseSlot`.
    fn can_use_slot(&self, _slot: EquipmentSlot) -> bool {
        true
    }
    /// `LivingEntity.getEquipmentSlotForItem`.
    fn get_equipment_slot_for_item(&self, stack: &ItemStack) -> EquipmentSlot {
        match equippable_slot(stack) {
            Some(s) if self.can_use_slot(s) => s,
            _ => EquipmentSlot::MainHand,
        }
    }
    /// `LivingEntity.isEquippableInSlot` (`Equippable.canBeEquippedBy` for the player entity type is not modelled:
    /// the vanilla equippables a player can hold all allow players).
    fn is_equippable_in_slot(&self, stack: &ItemStack, slot: EquipmentSlot) -> bool {
        match equippable_slot(stack) {
            None => slot == EquipmentSlot::MainHand && self.can_use_slot(EquipmentSlot::MainHand),
            Some(s) => slot == s && self.can_use_slot(s),
        }
    }
    /// `isWithinBlockInteractionRange(pos, buffer)`.
    fn is_within_block_interaction_range(&self, pos: BlockPos, buffer: f64) -> bool;
    /// Sends `ClientboundSetPlayerInventoryPacket(slot, stack)` (`Inventory.createInventoryUpdatePacket`).
    fn send_inventory_slot_update(&mut self, slot: usize, stack: ItemStack);
    /// The player's level (`None` for level-less test players).
    fn level(&mut self) -> Option<&mut ServerLevel> {
        None
    }
}

/// `Inventory.placeItemBackInInventory(stack, shouldSendSetSlotPacket, prediction)`.
pub fn place_item_back_in_inventory(player: &mut dyn MenuPlayer, stack: &mut ItemStack, should_send_set_slot_packet: bool, prediction: Prediction) {
    while !stack.is_empty() {
        let mut slot = player.inventory().get_slot_with_remaining_space(stack);
        if slot == -1 {
            slot = player.inventory().get_free_slot();
        }
        if slot == -1 {
            let rest = std::mem::take(stack);
            player.drop(rest, false, prediction);
            break;
        }
        let space = stack.get_max_stack_size() - crate::world::container::Container::get_item(player.inventory(), slot as usize).count();
        let mut part = stack.split(space);
        let infinite = player.has_infinite_materials();
        if player.inventory_mut().add_at(slot, &mut part, infinite) && should_send_set_slot_packet && player.is_server_player() {
            let updated = crate::world::container::Container::get_item(player.inventory(), slot as usize).copy();
            player.send_inventory_slot_update(slot as usize, updated);
        }
    }
}
