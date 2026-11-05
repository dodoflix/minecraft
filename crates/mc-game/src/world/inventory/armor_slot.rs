//! `ArmorSlot` and the `InventoryMenu` offhand slot subclass.
use super::{Slot, SlotBehavior, SlotEnv};
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::ItemStack;
use crate::world::item::ItemStackExt;

/// `ArmorSlot`: owner is always the menu's player (the player passed as `env.player`).
pub struct ArmorSlot {
    slot: EquipmentSlot,
}

impl ArmorSlot {
    pub fn new(slot: EquipmentSlot) -> Self {
        Self { slot }
    }
}

impl SlotBehavior for ArmorSlot {
    fn set_by_player(&self, slot: &Slot, env: &mut SlotEnv, stack: ItemStack, previous: &ItemStack) {
        env.player.on_equip_item(self.slot, previous, &stack);
        slot.set(env, stack);
    }
    fn get_max_stack_size(&self, _slot: &Slot, _env: &mut SlotEnv) -> i32 {
        1
    }
    fn may_place(&self, _slot: &Slot, env: &mut SlotEnv, stack: &ItemStack) -> bool {
        env.player.is_equippable_in_slot(stack, self.slot)
    }
    fn may_pickup(&self, slot: &Slot, env: &mut SlotEnv) -> bool {
        let item = slot.get_item(env);
        !(!item.is_empty() && !env.player.is_creative() && crate::world::item::enchantment_helper::has(&env.player.registries(), &item, crate::world::item::enchantment::enchantment_effect_components::EffectComponent::prevent_armor_change))
    }
}

/// The offhand slot of `InventoryMenu` (`owner.onEquipItem(OFFHAND, previous, new)` before the store).
pub struct OffhandSlot;

impl SlotBehavior for OffhandSlot {
    fn set_by_player(&self, slot: &Slot, env: &mut SlotEnv, stack: ItemStack, previous: &ItemStack) {
        env.player.on_equip_item(EquipmentSlot::OffHand, previous, &stack);
        slot.set(env, stack);
    }
}
