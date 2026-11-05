//! SEAM(EcEnchant): the `EnchantmentHelper` queries loot needs, over the item components until the typed enchantment
//! system lands (then these forward to it).

use mc_core::ident::Identifier;
use mc_protocol::components::values::basic::ENCHANTMENTS;
use mc_protocol::components::{Holder, ItemStack};

use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;

/// `EnchantmentHelper.getItemEnchantmentLevel(enchantment, stack)`.
pub fn get_item_enchantment_level(enchantment: &Identifier, stack: &ItemStack) -> i32 {
    stack.get(&ENCHANTMENTS).map_or(0, |e| e.get_level(&Holder::Reference(enchantment.clone())))
}

/// `EquipmentSlotGroup.test(slot)` by group name.
fn group_contains(group: &str, slot: EquipmentSlot) -> bool {
    match group {
        "any" => true,
        "mainhand" => slot == EquipmentSlot::MainHand,
        "offhand" => slot == EquipmentSlot::OffHand,
        "hand" => matches!(slot, EquipmentSlot::MainHand | EquipmentSlot::OffHand),
        "feet" => slot == EquipmentSlot::Feet,
        "legs" => slot == EquipmentSlot::Legs,
        "chest" => slot == EquipmentSlot::Chest,
        "head" => slot == EquipmentSlot::Head,
        "armor" => matches!(slot, EquipmentSlot::Feet | EquipmentSlot::Legs | EquipmentSlot::Chest | EquipmentSlot::Head),
        "body" => slot == EquipmentSlot::Body,
        "saddle" => slot == EquipmentSlot::Saddle,
        _ => false,
    }
}

/// `EnchantmentHelper.getEnchantmentLevel(enchantment, entity)`: the highest level among the items in the slots the
/// enchantment is active in (`definition.slots`).
pub fn get_enchantment_level(level: &ServerLevel, enchantment: &Identifier, entity: &dyn Entity) -> i32 {
    let Some(entry) = level.registries.registry("minecraft:enchantment").and_then(|r| r.entry_by_key(enchantment)) else { return 0 };
    let groups: Vec<&str> = match entry.json.get("definition").and_then(|d| d.get("slots")) {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|v| v.as_str()).collect(),
        _ => Vec::new(),
    };
    let mut max = 0;
    for slot in EquipmentSlot::VALUES {
        if groups.iter().any(|g| group_contains(g, slot)) {
            let stack = entity.get_item_by_slot(slot);
            if !stack.is_empty() {
                max = max.max(get_item_enchantment_level(enchantment, &stack));
            }
        }
    }
    max
}
