//! TEMPORARY local `ItemStack` with the vanilla API shape (snake_case vanilla names). `ComponentsAgent` owns the real
//! type (`mc_protocol::components::ItemStack`, typed data components, codecs, `HashedStack`); when it lands this file is
//! deleted and `world::item` re-exports the real type — every caller imports `crate::world::item::ItemStack`.
//!
//! Components are modelled as a patch over the item's default components (`mc_data` JSON) with vanilla's
//! `PatchedDataComponentMap` normalisation (a value equal to the prototype's is not stored), which is all the
//! menu/inventory algorithms need (`isSameItemSameComponents`, damage, stack size, enchantments, custom name).

use mc_data::Item;
use mc_nbt::CompoundTag;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ItemStack {
    item: Item,
    count: i32,
    /// `None` = component removed (`!minecraft:x`).
    patch: BTreeMap<String, Option<Value>>,
}

impl PartialEq for ItemStack {
    /// Component-map equality of two stacks (`PatchedDataComponentMap.equals`) plus item and raw count; vanilla uses
    /// reference identity for `==` — callers that need `ItemStack.matches` call [`ItemStack::matches`].
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item && self.count == other.count && self.patch == other.patch
    }
}

impl Default for ItemStack {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ItemStack {
    pub const EMPTY: ItemStack = ItemStack { item: Item::AIR, count: 0, patch: BTreeMap::new() };

    pub fn empty() -> Self {
        Self::EMPTY
    }

    pub fn new(item: Item, count: i32) -> Self {
        Self { item, count, patch: BTreeMap::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.item == Item::AIR || self.count <= 0
    }

    /// `getItem()` (AIR when empty).
    pub fn item(&self) -> Item {
        if self.is_empty() { Item::AIR } else { self.item }
    }

    pub fn is(&self, item: Item) -> bool {
        self.item() == item
    }

    pub fn count(&self) -> i32 {
        if self.is_empty() { 0 } else { self.count }
    }

    pub fn set_count(&mut self, count: i32) {
        self.count = count;
    }

    pub fn grow(&mut self, amount: i32) {
        self.set_count(self.count.wrapping_add(amount));
    }

    pub fn shrink(&mut self, amount: i32) {
        self.grow(amount.wrapping_neg());
    }

    pub fn copy(&self) -> ItemStack {
        if self.is_empty() { Self::EMPTY } else { self.clone() }
    }

    pub fn copy_with_count(&self, count: i32) -> ItemStack {
        if self.is_empty() {
            Self::EMPTY
        } else {
            let mut c = self.clone();
            c.count = count;
            c
        }
    }

    pub fn copy_and_clear(&mut self) -> ItemStack {
        if self.is_empty() {
            Self::EMPTY
        } else {
            let copy = self.copy();
            self.set_count(0);
            copy
        }
    }

    pub fn split(&mut self, amount: i32) -> ItemStack {
        let real = amount.min(self.count());
        let result = self.copy_with_count(real);
        self.shrink(real);
        result
    }

    pub fn limit_size(&mut self, max: i32) {
        if !self.is_empty() && self.count() > max {
            self.set_count(max);
        }
    }

    pub fn is_same_item(a: &ItemStack, b: &ItemStack) -> bool {
        a.is(b.item())
    }

    pub fn is_same_item_same_components(a: &ItemStack, b: &ItemStack) -> bool {
        if !a.is(b.item()) {
            false
        } else if a.is_empty() && b.is_empty() {
            true
        } else {
            a.patch == b.patch
        }
    }

    /// `ItemStack.matches`.
    pub fn matches(a: &ItemStack, b: &ItemStack) -> bool {
        if a.is_empty() && b.is_empty() {
            true
        } else {
            !a.is_empty() && !b.is_empty() && Self::matches_non_empty(a, b)
        }
    }

    fn matches_non_empty(a: &ItemStack, b: &ItemStack) -> bool {
        a.count() == b.count() && a.is(b.item()) && a.patch == b.patch
    }

    // ---- components (JSON shim) ----

    /// Effective component `key` (`minecraft:damage`): patch over the item's defaults.
    pub fn get_json(&self, key: &str) -> Option<Value> {
        match self.patch.get(key) {
            Some(v) => v.clone(),
            None => self.item.default_components().get(key).cloned(),
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.get_json(key).is_some()
    }

    /// `set(type, value)`; `None` = `remove(type)`.
    pub fn set_json(&mut self, key: &str, value: Option<Value>) {
        let proto = self.item.default_components().get(key).cloned();
        if proto == value {
            self.patch.remove(key);
        } else {
            self.patch.insert(key.to_owned(), value);
        }
    }

    pub fn get_max_stack_size(&self) -> i32 {
        self.get_json("minecraft:max_stack_size").and_then(|v| v.as_i64()).map_or(1, |v| v as i32)
    }

    pub fn is_damageable_item(&self) -> bool {
        self.has("minecraft:max_damage") && !self.has("minecraft:unbreakable") && self.has("minecraft:damage")
    }

    pub fn get_damage_value(&self) -> i32 {
        let max = self.get_max_damage();
        let d = self.get_json("minecraft:damage").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        d.clamp(0, max)
    }

    pub fn set_damage_value(&mut self, damage: i32) {
        let max = self.get_max_damage();
        self.set_json("minecraft:damage", Some(Value::from(damage.clamp(0, max))));
    }

    pub fn get_max_damage(&self) -> i32 {
        self.get_json("minecraft:max_damage").and_then(|v| v.as_i64()).unwrap_or(0) as i32
    }

    pub fn is_damaged(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() > 0
    }

    pub fn is_stackable(&self) -> bool {
        self.get_max_stack_size() > 1 && (!self.is_damageable_item() || !self.is_damaged())
    }

    pub fn is_enchanted(&self) -> bool {
        self.get_json("minecraft:enchantments").is_some_and(|v| v.as_object().is_some_and(|o| !o.is_empty()))
    }

    // ---- NBT (`ItemStack.MAP_CODEC`: `id`, `count`, optional `components`) ----

    /// Writes `id`/`count`/`components` into `tag` (the `MAP_CODEC` fields, as used by `ItemStackWithSlot`).
    pub fn save_into(&self, tag: &mut CompoundTag) {
        tag.put_string("id", self.item.name());
        tag.put_int("count", self.count());
        // Components are not persisted by the shim (the real `DataComponentPatch` codec lands with ComponentsAgent).
    }

    /// Reads the `MAP_CODEC` fields; `None` for an unknown item or a count outside `1..=99` (vanilla's `ItemStack.CODEC`
    /// validation) — the caller drops the entry like a failed decode.
    pub fn load_from(tag: &CompoundTag) -> Option<ItemStack> {
        let item = Item::by_name(tag.get_string("id")?)?;
        let count = tag.get_int_or("count", 1);
        if !(1..=99).contains(&count) || item == Item::AIR {
            return None;
        }
        Some(ItemStack::new(item, count))
    }
}

/// TEMPORARY stand-in for `net.minecraft.network.HashedStack` (the real one hashes the component patch with CRC32C and
/// lives in `mc_protocol::components`): only item and count are compared here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HashedStack {
    Empty,
    Actual { item: Item, count: i32 },
}

impl HashedStack {
    pub fn create(stack: &ItemStack) -> HashedStack {
        if stack.is_empty() { HashedStack::Empty } else { HashedStack::Actual { item: stack.item(), count: stack.count() } }
    }

    pub fn matches(&self, stack: &ItemStack) -> bool {
        match self {
            HashedStack::Empty => stack.is_empty(),
            HashedStack::Actual { item, count } => *count == stack.count() && *item == stack.item(),
        }
    }
}
