//! `ItemEnchantments.Mutable` and the stack-level enchantment accessors.

use mc_core::ident::Identifier;
use mc_protocol::components::{ENCHANTMENTS, EnchantmentHolder, Holder, ItemEnchantments, STORED_ENCHANTMENTS};

use crate::world::item::ItemStack;

/// `ItemEnchantments.Mutable`.
#[derive(Clone, Debug, Default)]
pub struct ItemEnchantmentsMutable {
    entries: Vec<(EnchantmentHolder, i32)>,
}

impl ItemEnchantmentsMutable {
    /// `new Mutable(enchantments)`.
    pub fn new(enchantments: &ItemEnchantments) -> ItemEnchantmentsMutable {
        ItemEnchantmentsMutable { entries: enchantments.entries().to_vec() }
    }

    fn position(&self, enchantment: &EnchantmentHolder) -> Option<usize> {
        self.entries.iter().position(|(k, _)| k == enchantment)
    }

    /// `set(enchantment, level)`: a level <= 0 removes it, levels are capped at 255.
    pub fn set(&mut self, enchantment: &EnchantmentHolder, level: i32) {
        match (self.position(enchantment), level <= 0) {
            (Some(at), true) => {
                self.entries.remove(at);
            }
            (None, true) => {}
            (Some(at), false) => self.entries[at].1 = level.min(255),
            (None, false) => self.entries.push((enchantment.clone(), level.min(255))),
        }
    }

    /// `upgrade(enchantment, level)`: keeps the higher level.
    pub fn upgrade(&mut self, enchantment: &EnchantmentHolder, level: i32) {
        if level > 0 {
            let level = level.min(255);
            match self.position(enchantment) {
                Some(at) => self.entries[at].1 = self.entries[at].1.max(level),
                None => self.entries.push((enchantment.clone(), level)),
            }
        }
    }

    /// `removeIf(predicate)` over the enchantments.
    pub fn remove_if(&mut self, mut predicate: impl FnMut(&EnchantmentHolder) -> bool) {
        self.entries.retain(|(k, _)| !predicate(k));
    }

    /// `getLevel(enchantment)`: 0 when absent.
    pub fn get_level(&self, enchantment: &EnchantmentHolder) -> i32 {
        self.position(enchantment).map_or(0, |at| self.entries[at].1)
    }

    /// `keySet()`.
    pub fn key_set(&self) -> impl Iterator<Item = &EnchantmentHolder> {
        self.entries.iter().map(|(k, _)| k)
    }

    /// `toImmutable()`.
    pub fn to_immutable(&self) -> ItemEnchantments {
        ItemEnchantments::new(self.entries.clone())
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
    super::super::enchantment_helper::update_enchantments(stack, |m| m.upgrade(&holder(enchantment), level));
}

/// The component an enchantment set of `stack` lives in (`getComponentType`): enchanted books store theirs separately.
pub fn is_book(stack: &ItemStack) -> bool {
    stack.is(mc_data::Item::ENCHANTED_BOOK)
}

pub(crate) fn component_key(stack: &ItemStack) -> mc_protocol::components::ComponentKey<ItemEnchantments> {
    if is_book(stack) { STORED_ENCHANTMENTS } else { ENCHANTMENTS }
}
