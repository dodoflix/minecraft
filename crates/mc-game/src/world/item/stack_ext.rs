//! Helpers on top of `mc_protocol::components::ItemStack` that the menu/inventory/item code needs
//! (`ItemStack` methods that depend on game types, or that the value type does not carry).

use super::enchantment_helper;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::{ENCHANTMENTS, ItemStack};

pub trait ItemStackExt {
    /// `isEnchanted()`.
    fn is_enchanted(&self) -> bool;
    /// `ItemStack.MAP_CODEC` fields (`id`, `count`, `components`) merged into `tag`.
    fn save_into(&self, tag: &mut CompoundTag);
    /// `consume(amount, owner)`: shrink unless the owner has infinite materials.
    fn consume(&mut self, amount: i32, owner_has_infinite_materials: bool);
    /// `consumeAndReturn(amount, owner)`.
    fn consume_and_return(&mut self, amount: i32, owner_has_infinite_materials: bool) -> ItemStack;
    /// `isBroken()`.
    fn is_broken(&self) -> bool;
    /// `nextDamageWillBreak()`.
    fn next_damage_will_break(&self) -> bool;
    /// `EnchantmentHelper.has(stack, effectComponent)` (e.g. `minecraft:prevent_armor_change`).
    fn has_enchantment_effect(&self, effect_component: &str) -> bool;
}

impl ItemStackExt for ItemStack {
    fn is_enchanted(&self) -> bool {
        self.get(&ENCHANTMENTS).is_some_and(|e| !e.is_empty())
    }

    fn save_into(&self, tag: &mut CompoundTag) {
        if let Ok(Tag::Compound(c)) = self.to_tag().into_result() {
            tag.merge(&c);
        }
    }

    fn consume(&mut self, amount: i32, owner_has_infinite_materials: bool) {
        if !owner_has_infinite_materials {
            self.shrink(amount);
        }
    }

    fn consume_and_return(&mut self, amount: i32, owner_has_infinite_materials: bool) -> ItemStack {
        let split = self.copy_with_count(amount);
        self.consume(amount, owner_has_infinite_materials);
        split
    }

    fn is_broken(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() >= self.get_max_damage()
    }

    fn next_damage_will_break(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() >= self.get_max_damage() - 1
    }

    fn has_enchantment_effect(&self, effect_component: &str) -> bool {
        enchantment_helper::has(self, effect_component)
    }
}

/// Decodes the `MAP_CODEC` fields of `tag` (`None` = undecodable entry: unknown item, count outside 1..=99, ...).
pub fn load_item_stack(tag: &CompoundTag) -> Option<ItemStack> {
    ItemStack::from_tag(&Tag::Compound(tag.clone())).into_result().ok()
}
