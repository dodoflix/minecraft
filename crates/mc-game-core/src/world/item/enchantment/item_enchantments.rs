//! `ItemEnchantments.Mutable` and the stack-level enchantment accessors.

use mc_core::ident::Identifier;
use mc_protocol::components::{
    ENCHANTMENTS, EnchantmentHolder, EnchantmentMap, Holder, ItemEnchantments, STORED_ENCHANTMENTS,
    enchantment_map,
};

use crate::world::item::ItemStack;

/// `ItemEnchantments.Mutable`: its own `Object2IntOpenHashMap` (a fresh default-size table the enchantments are put into).
#[derive(Clone, Debug)]
pub struct ItemEnchantmentsMutable {
    enchantments: EnchantmentMap,
}

impl Default for ItemEnchantmentsMutable {
    fn default() -> Self {
        ItemEnchantmentsMutable {
            enchantments: enchantment_map(16),
        }
    }
}

impl ItemEnchantmentsMutable {
    /// `new Mutable(enchantments)`: `putAll` into the default table.
    pub fn new(enchantments: &ItemEnchantments) -> ItemEnchantmentsMutable {
        let mut mutable = ItemEnchantmentsMutable::default();
        mutable.enchantments.put_all(
            enchantments
                .entries()
                .map(|(k, level)| (k.clone(), level))
                .collect(),
        );
        mutable
    }

    /// `set(enchantment, level)`: a level <= 0 removes it, levels are capped at 255.
    pub fn set(&mut self, enchantment: &EnchantmentHolder, level: i32) {
        if level <= 0 {
            self.enchantments.remove(enchantment);
        } else {
            self.enchantments.put(enchantment.clone(), level.min(255));
        }
    }

    /// `upgrade(enchantment, level)`: keeps the higher level.
    pub fn upgrade(&mut self, enchantment: &EnchantmentHolder, level: i32) {
        if level > 0 {
            let level = level.min(255);
            let merged = self.get_level(enchantment).max(level);
            self.enchantments.put(enchantment.clone(), merged);
        }
    }

    /// `removeIf(predicate)` over the enchantments (`keySet().removeIf`: the table does not shrink).
    pub fn remove_if(&mut self, mut predicate: impl FnMut(&EnchantmentHolder) -> bool) {
        let doomed: Vec<EnchantmentHolder> =
            self.key_set().filter(|k| predicate(k)).cloned().collect();
        for k in doomed {
            self.enchantments.remove_during_iteration(&k);
        }
    }

    /// `getLevel(enchantment)`: 0 when absent.
    pub fn get_level(&self, enchantment: &EnchantmentHolder) -> i32 {
        self.enchantments.get(enchantment).copied().unwrap_or(0)
    }

    /// `keySet()` in table order.
    pub fn key_set(&self) -> impl Iterator<Item = &EnchantmentHolder> {
        self.enchantments.iter().map(|(k, _)| k)
    }

    /// `toImmutable()`.
    pub fn to_immutable(&self) -> ItemEnchantments {
        ItemEnchantments::from_map(self.enchantments.clone()).unwrap_or_else(|e| panic!("{e}"))
    }
}

/// `Holder<Enchantment>` of a registry key.
pub fn holder(enchantment: &Identifier) -> EnchantmentHolder {
    Holder::Reference(enchantment.clone())
}

/// `ItemStack.getEnchantments()`.
pub fn get_enchantments(stack: &ItemStack) -> ItemEnchantments {
    stack.get(&ENCHANTMENTS).cloned().unwrap_or_default()
}

/// `ItemStack.enchant(enchantment, level)` (`EnchantmentHelper.updateEnchantments` + `upgrade`).
pub fn enchant(stack: &mut ItemStack, enchantment: &Identifier, level: i32) {
    super::super::enchantment_helper::update_enchantments(stack, |m| {
        m.upgrade(&holder(enchantment), level)
    });
}

/// The component an enchantment set of `stack` lives in (`getComponentType`): enchanted books store theirs separately.
pub fn is_book(stack: &ItemStack) -> bool {
    stack.is(mc_data::Item::ENCHANTED_BOOK)
}

pub(crate) fn component_key(
    stack: &ItemStack,
) -> mc_protocol::components::ComponentKey<ItemEnchantments> {
    if is_book(stack) {
        STORED_ENCHANTMENTS
    } else {
        ENCHANTMENTS
    }
}
