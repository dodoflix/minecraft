//! `EnchantmentInstance`.

use mc_core::ident::Identifier;

/// `EnchantmentInstance(enchantment, level)`; the enchantment is its registry key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnchantmentInstance {
    pub enchantment: Identifier,
    pub level: i32,
}
