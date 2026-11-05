//! `EnchantedItemInUse`: the stack an enchantment effect runs for, the slot it is in and who holds it.

use crate::server::level::ServerLevel;
use crate::world::entity::EntityId;
use crate::world::entity::equipment::EquipmentSlot;
use crate::world::item::ItemStack;

/// What happens when the stack breaks (`onBreak`).
pub enum OnBreak {
    /// `owner.onEquippedItemBroken(item, inSlot)`.
    OwnerEquipped,
    /// A caller supplied `Consumer<ItemStack>`.
    Callback(Box<dyn FnMut(&mut ServerLevel, &ItemStack) + Send>),
}

/// `EnchantedItemInUse(itemStack, inSlot, owner, onBreak)`. The stack is a working copy of the one held in `in_slot` of
/// `owner` (or of a weapon in hand); effects change it in place and the iteration writes it back when [`modified`] is set.
pub struct EnchantedItemInUse {
    pub item_stack: ItemStack,
    pub in_slot: Option<EquipmentSlot>,
    pub owner: Option<EntityId>,
    pub on_break: OnBreak,
    /// Set by effects that changed `item_stack` (`ChangeItemDamage`).
    pub modified: bool,
}

impl EnchantedItemInUse {
    /// `new EnchantedItemInUse(itemStack, inSlot, owner)`: breaking notifies the owner.
    pub fn new(item_stack: ItemStack, in_slot: Option<EquipmentSlot>, owner: EntityId) -> EnchantedItemInUse {
        EnchantedItemInUse { item_stack, in_slot, owner: Some(owner), on_break: OnBreak::OwnerEquipped, modified: false }
    }

    /// `new EnchantedItemInUse(itemStack, inSlot, owner, onBreak)`.
    pub fn with_on_break(item_stack: ItemStack, in_slot: Option<EquipmentSlot>, owner: Option<EntityId>, on_break: OnBreak) -> EnchantedItemInUse {
        EnchantedItemInUse { item_stack, in_slot, owner, on_break, modified: false }
    }
}
