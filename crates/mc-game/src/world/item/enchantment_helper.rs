//! `EnchantmentHelper` queries on a stack's enchantments, answered from the enchantment definitions.
//!
//! Only the built-in definitions of the vanilla data pack are consulted (custom enchantments of world data packs are
//! not loaded into a registry yet); `processDurabilityChange` and the other value-effect pipelines need the
//! loot-context machinery of the enchantment slice and are hooks, see [`set_durability_processor`].

use mc_core::ident::Identifier;
use mc_protocol::components::{ENCHANTMENTS, ItemStack};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The `effects` object of a built-in enchantment (`data/<ns>/enchantment/<path>.json`), cached.
fn effects_of(name: &Identifier) -> Option<&'static Value> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<&'static Value>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = name.to_string();
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = guard.get(&key) {
        return *v;
    }
    let parsed = mc_data::datapack::read_data(name.namespace(), &format!("enchantment/{}.json", name.path()))
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v.get("effects").cloned())
        // Leaked once per enchantment (bounded by the number of definitions): keeps the cache entries `'static`.
        .map(|v| &*Box::leak(Box::new(v)));
    guard.insert(key, parsed);
    parsed
}

/// `EnchantmentHelper.has(stack, effectComponent)`: any enchantment on the stack whose definition has the effect.
pub fn has(stack: &ItemStack, effect_component: &str) -> bool {
    let Some(enchantments) = stack.get(&ENCHANTMENTS) else { return false };
    enchantments.entries().iter().any(|(holder, _)| {
        holder.name().and_then(effects_of).is_some_and(|effects| effects.get(effect_component).is_some())
    })
}

/// `EnchantmentHelper.processDurabilityChange(level, stack, amount)`.
pub type DurabilityProcessor = fn(stack: &ItemStack, amount: i32) -> i32;

static DURABILITY: OnceLock<DurabilityProcessor> = OnceLock::new();

/// Installs the enchantment slice's `ITEM_DAMAGE` effect pipeline (unbreaking...). Without it durability changes pass
/// through unchanged, which is exact for stacks without damage-affecting enchantments.
pub fn set_durability_processor(p: DurabilityProcessor) {
    let _ = DURABILITY.set(p);
}

pub fn process_durability_change(stack: &ItemStack, amount: i32) -> i32 {
    DURABILITY.get().map_or(amount, |p| p(stack, amount))
}
