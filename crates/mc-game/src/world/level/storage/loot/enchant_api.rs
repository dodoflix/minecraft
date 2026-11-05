//! SEAM(EcEnchant): the `EnchantmentHelper` / `Enchantment` queries loot needs, answered from the `enchantment` registry's
//! definitions (JSON) and the item components. Replace the bodies by the typed enchantment system when it lands.

use mc_core::ident::Identifier;
use mc_core::math::java::round_f32;
use mc_core::random::RandomSource;
use mc_data::Item;
use mc_protocol::components::values::basic::{ENCHANTABLE, ENCHANTMENTS, ItemEnchantments, STORED_ENCHANTMENTS};
use mc_protocol::components::{Holder, HolderSet, ItemStack, NoDirect};
use serde_json::Value;

use crate::registry::{RegistryAccess, keys};
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::equipment::EquipmentSlot;

fn component_is_stored(stack: &ItemStack) -> bool {
    stack.is(Item::ENCHANTED_BOOK)
}

/// `stack.get(ENCHANTMENTS / STORED_ENCHANTMENTS)` as (id, level).
pub fn get_stack_enchantments(stack: &ItemStack) -> Vec<(Identifier, i32)> {
    let component = if component_is_stored(stack) { stack.get(&STORED_ENCHANTMENTS) } else { stack.get(&ENCHANTMENTS) };
    component
        .map(|e| e.entries().iter().filter_map(|(h, l)| h.name().map(|n| (n.clone(), *l))).collect())
        .unwrap_or_default()
}

/// `EnchantmentHelper.getItemEnchantmentLevel(enchantment, stack)`.
pub fn get_item_enchantment_level(enchantment: &Identifier, stack: &ItemStack) -> i32 {
    stack.get(&ENCHANTMENTS).map_or(0, |e| e.get_level(&Holder::Reference(enchantment.clone())))
}

/// `ItemEnchantments.Mutable`.
pub struct EnchantmentsMutable {
    entries: Vec<(Identifier, i32)>,
}

impl EnchantmentsMutable {
    pub fn get_level(&self, enchantment: &Identifier) -> i32 {
        self.entries.iter().find(|(e, _)| e == enchantment).map_or(0, |(_, l)| *l)
    }
    /// `set(enchantment, level)`: a level of `0` or less removes it.
    pub fn set(&mut self, enchantment: &Identifier, level: i32) {
        self.entries.retain(|(e, _)| e != enchantment);
        if level > 0 {
            self.entries.push((enchantment.clone(), level));
        }
    }
    /// `upgrade(enchantment, level)`.
    pub fn upgrade(&mut self, enchantment: &Identifier, level: i32) {
        if level > 0 && level > self.get_level(enchantment) {
            self.set(enchantment, level);
        }
    }
}

/// `EnchantmentHelper.updateEnchantments(stack, consumer)`: a stack without the component is left alone.
pub fn update_enchantments(stack: &mut ItemStack, f: impl FnOnce(&mut EnchantmentsMutable)) {
    let stored = component_is_stored(stack);
    let current = if stored { stack.get(&STORED_ENCHANTMENTS) } else { stack.get(&ENCHANTMENTS) };
    let Some(current) = current else { return };
    let mut mutable = EnchantmentsMutable { entries: current.entries().iter().filter_map(|(h, l)| h.name().map(|n| (n.clone(), *l))).collect() };
    f(&mut mutable);
    let updated = ItemEnchantments::new(mutable.entries.into_iter().map(|(e, l)| (Holder::Reference(e), l)).collect());
    if stored {
        stack.set(STORED_ENCHANTMENTS, updated);
    } else {
        stack.set(ENCHANTMENTS, updated);
    }
}

/// `ItemStack.enchant(enchantment, level)`.
pub fn enchant(stack: &mut ItemStack, enchantment: &Identifier, level: i32) {
    update_enchantments(stack, |e| e.upgrade(enchantment, level));
}

// ---- registry data ----------------------------------------------------------------------------------------------------------------------

fn definition<'a>(registries: &'a RegistryAccess, id: &Identifier) -> Option<&'a Value> {
    registries.registry(keys::ENCHANTMENT)?.entry_by_key(id).map(|e| &*e.json)
}

/// The ids of a holder set (`#tag`, an id or a list of ids) of `registry`, tags expanded in tag order.
fn json_holder_set(registries: &RegistryAccess, registry: &str, set: &Value) -> Vec<Identifier> {
    match set {
        Value::String(s) => match s.strip_prefix('#') {
            Some(tag) => Identifier::try_parse(tag).map(|t| tag_names(registries, registry, &t)).unwrap_or_default(),
            None => Identifier::try_parse(s).into_iter().collect(),
        },
        Value::Array(list) => list.iter().filter_map(|v| v.as_str().and_then(Identifier::try_parse)).collect(),
        _ => Vec::new(),
    }
}

fn tag_names(registries: &RegistryAccess, registry: &str, tag: &Identifier) -> Vec<Identifier> {
    let Some(r) = registries.registry(registry) else { return Vec::new() };
    r.tag(tag).unwrap_or(&[]).iter().filter_map(|id| r.name_of(*id)).collect()
}

/// The ids of `set` (a decoded `HolderSet`) in `registry`.
pub fn holder_set_ids(registries: &RegistryAccess, registry: &str, set: &HolderSet<NoDirect>) -> Vec<Identifier> {
    match set {
        HolderSet::Named(tag) => tag_names(registries, registry, tag),
        HolderSet::Direct(v) => v.iter().filter_map(|h| h.name().cloned()).collect(),
    }
}

fn item_in(registries: &RegistryAccess, set: &Value, stack: &ItemStack) -> bool {
    let name = stack.item().name();
    json_holder_set(registries, "minecraft:item", set).iter().any(|id| id.to_string() == name)
}

/// `Enchantment.canEnchant(stack)` (`supportedItems().contains(item)`).
pub fn can_enchant(registries: &RegistryAccess, id: &Identifier, stack: &ItemStack) -> bool {
    definition(registries, id).and_then(|d| d.get("supported_items")).is_some_and(|s| item_in(registries, s, stack))
}

fn is_primary_item(registries: &RegistryAccess, id: &Identifier, stack: &ItemStack) -> bool {
    let Some(d) = definition(registries, id) else { return false };
    d.get("supported_items").is_some_and(|s| item_in(registries, s, stack)) && d.get("primary_items").is_none_or(|p| item_in(registries, p, stack))
}

fn max_level(registries: &RegistryAccess, id: &Identifier) -> i32 {
    definition(registries, id).and_then(|d| d.get("max_level")).and_then(Value::as_i64).unwrap_or(1) as i32
}

/// `(getMinLevel(), getMaxLevel())`.
pub fn min_max_level(registries: &RegistryAccess, id: &Identifier) -> (i32, i32) {
    (1, max_level(registries, id))
}

/// `LevelBasedValue.Linear` costs (`min_cost` / `max_cost`): `base + per_level_above_first * (level - 1)`.
fn cost(registries: &RegistryAccess, id: &Identifier, field: &str, level: i32) -> i32 {
    let v = definition(registries, id).and_then(|d| d.get(field));
    let get = |k: &str| v.and_then(|v| v.get(k)).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    (get("base") + get("per_level_above_first") * (level - 1) as f32) as i32
}

fn are_compatible(registries: &RegistryAccess, a: &Identifier, b: &Identifier) -> bool {
    let excludes = |x: &Identifier, y: &Identifier| {
        definition(registries, x)
            .and_then(|d| d.get("exclusive_set"))
            .is_some_and(|s| json_holder_set(registries, keys::ENCHANTMENT, s).contains(y))
    };
    a != b && !excludes(a, b) && !excludes(b, a)
}

/// `WeightedRandom.getRandomItem(random, items, weight)`.
fn random_weighted(random: &mut dyn RandomSource, items: &[(Identifier, i32)], registries: &RegistryAccess) -> Option<(Identifier, i32)> {
    let weight = |e: &(Identifier, i32)| definition(registries, &e.0).and_then(|d| d.get("weight")).and_then(Value::as_i64).unwrap_or(1) as i32;
    let total: i32 = items.iter().map(weight).fold(0i32, i32::wrapping_add);
    if total == 0 {
        return None;
    }
    let mut selection = random.next_int_bounded(total);
    for item in items {
        selection -= weight(item);
        if selection < 0 {
            return Some(item.clone());
        }
    }
    None
}

/// `EnchantmentHelper.getAvailableEnchantmentResults(value, stack, source)`.
fn available_results(registries: &RegistryAccess, value: i32, stack: &ItemStack, source: &[Identifier]) -> Vec<(Identifier, i32)> {
    let is_book = stack.is(Item::BOOK);
    let mut results = Vec::new();
    for id in source {
        if !(is_primary_item(registries, id, stack) || is_book) {
            continue;
        }
        let (min, max) = min_max_level(registries, id);
        for level in (min..=max).rev() {
            if value >= cost(registries, id, "min_cost", level) && value <= cost(registries, id, "max_cost", level) {
                results.push((id.clone(), level));
                break;
            }
        }
    }
    results
}

/// `EnchantmentHelper.selectEnchantment(random, stack, enchantmentCost, source)`.
fn select_enchantment(random: &mut dyn RandomSource, stack: &ItemStack, mut enchantment_cost: i32, registries: &RegistryAccess, source: &[Identifier]) -> Vec<(Identifier, i32)> {
    let mut results: Vec<(Identifier, i32)> = Vec::new();
    let Some(enchantable) = stack.get(&ENCHANTABLE) else { return results };
    let quarter = enchantable.value / 4 + 1;
    enchantment_cost = enchantment_cost.wrapping_add(1 + random.next_int_bounded(quarter) + random.next_int_bounded(quarter));
    let random_span = (random.next_float() + random.next_float() - 1.0) * 0.15;
    enchantment_cost = round_f32(enchantment_cost as f32 + enchantment_cost as f32 * random_span).max(1);
    let mut enchantments = available_results(registries, enchantment_cost, stack, source);
    if !enchantments.is_empty() {
        results.extend(random_weighted(random, &enchantments, registries));
        while random.next_int_bounded(50) <= enchantment_cost {
            if let Some(last) = results.last() {
                let last = last.0.clone();
                enchantments.retain(|e| are_compatible(registries, &last, &e.0));
            }
            if enchantments.is_empty() {
                break;
            }
            results.extend(random_weighted(random, &enchantments, registries));
            enchantment_cost /= 2;
        }
    }
    results
}

/// `EnchantmentHelper.enchantItem(random, stack, enchantmentCost, registryAccess, options)`.
pub fn enchant_item(
    random: &mut dyn RandomSource,
    mut stack: ItemStack,
    enchantment_cost: i32,
    registries: &RegistryAccess,
    options: Option<&HolderSet<NoDirect>>,
) -> ItemStack {
    let source: Vec<Identifier> = match options {
        Some(set) => holder_set_ids(registries, keys::ENCHANTMENT, set),
        None => registries.registry(keys::ENCHANTMENT).map(|r| r.names()).unwrap_or_default(),
    };
    let enchants = select_enchantment(random, &stack, enchantment_cost, registries, &source);
    if stack.is(Item::BOOK) {
        stack = ItemStack::new(Item::ENCHANTED_BOOK);
    }
    for (id, level) in enchants {
        enchant(&mut stack, &id, level);
    }
    stack
}

// ---- entities ---------------------------------------------------------------------------------------------------------------------------

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
    let Some(d) = definition(&level.registries, enchantment) else { return 0 };
    let groups: Vec<&str> = match d.get("slots") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
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
