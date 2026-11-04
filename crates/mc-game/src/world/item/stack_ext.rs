//! Helpers on top of `mc_protocol::components::ItemStack` that the menu/inventory/item code needs.
//!
//! Until ComponentsAgent has typed values for every component family (so far only `max_stack_size`, `max_damage`,
//! `damage`, `unbreakable`), the others are read as JSON from the item's *prototype* (`mc_data` default components).
//! That is exact for every stack that can exist today: a patch can only contain implemented components, so
//! non-implemented ones always equal the prototype. Replace the `*_json` accessors with typed `get` calls as the
//! component families land.

use mc_data::Item;
use mc_nbt::{CompoundTag, Tag};
use mc_protocol::components::ItemStack;
use serde_json::Value;
use std::sync::OnceLock;

fn prototype_json(item: Item) -> &'static Value {
    static CACHE: OnceLock<Vec<OnceLock<Value>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| (0..Item::registry().len()).map(|_| OnceLock::new()).collect());
    cache[item.id() as usize].get_or_init(|| item.default_components())
}

pub trait ItemStackExt {
    /// Prototype component `key` (`minecraft:tool`) as JSON; `None` for the empty stack.
    fn get_json(&self, key: &str) -> Option<&'static Value>;
    fn has_json(&self, key: &str) -> bool {
        self.get_json(key).is_some()
    }
    /// `isEnchanted()`.
    fn is_enchanted(&self) -> bool;
    /// `ItemStack.MAP_CODEC` fields (`id`, `count`, `components`) merged into `tag`.
    fn save_into(&self, tag: &mut CompoundTag);
}

impl ItemStackExt for ItemStack {
    fn get_json(&self, key: &str) -> Option<&'static Value> {
        if self.is_empty() { None } else { prototype_json(self.item()).get(key) }
    }

    fn is_enchanted(&self) -> bool {
        self.get_json("minecraft:enchantments").is_some_and(|v| v.as_object().is_some_and(|o| !o.is_empty()))
    }

    fn save_into(&self, tag: &mut CompoundTag) {
        if let Ok(Tag::Compound(c)) = self.to_tag().into_result() {
            tag.merge(&c);
        }
    }
}

/// Decodes the `MAP_CODEC` fields of `tag` (`None` = undecodable entry: unknown item, count outside 1..=99, ...).
pub fn load_item_stack(tag: &CompoundTag) -> Option<ItemStack> {
    ItemStack::from_tag(&Tag::Compound(tag.clone())).into_result().ok()
}
