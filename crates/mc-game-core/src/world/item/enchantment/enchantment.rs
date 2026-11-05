//! `Enchantment`: the data-driven definition (`enchantment` registry entry).

use mc_core::ident::Identifier;
use mc_data::Item;
use mc_dfu::{Codec, INT, record};
use mc_protocol::components::{EquipmentSlotGroup, HolderSet, NoDirect, RegistrySet, component_codec};
use mc_text::Component;

use super::codecs::holder_set_codec;
use super::enchantment_effect_components::EnchantmentEffects;
use crate::registry::RegistryAccess;
use crate::registry::keys::ENCHANTMENT as ENCHANTMENT_REGISTRY;
use crate::world::entity::equipment::{EquipmentSlot, EquipmentSlotType};
use crate::world::item::ItemStack;

/// `Enchantment.MAX_LEVEL`.
pub const MAX_LEVEL: i32 = 255;

/// `Enchantment.Cost`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cost {
    pub base: i32,
    pub per_level_above_first: i32,
}

impl Cost {
    /// `Enchantment.constantCost(base)`.
    pub fn constant(base: i32) -> Cost {
        Cost { base, per_level_above_first: 0 }
    }

    /// `Enchantment.dynamicCost(base, perLevel)`.
    pub fn dynamic(base: i32, per_level_above_first: i32) -> Cost {
        Cost { base, per_level_above_first }
    }

    /// `calculate(level)`.
    pub fn calculate(&self, level: i32) -> i32 {
        self.base.wrapping_add(self.per_level_above_first.wrapping_mul(level.wrapping_sub(1)))
    }

    pub fn codec() -> Codec<Cost> {
        record!(Cost { base: INT.field_of("base"), per_level_above_first: INT.field_of("per_level_above_first") }).codec()
    }
}

/// `Enchantment.EnchantmentDefinition`.
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentDefinition {
    pub supported_items: RegistrySet<Item>,
    pub primary_items: Option<RegistrySet<Item>>,
    pub weight: i32,
    pub max_level: i32,
    pub min_cost: Cost,
    pub max_cost: Cost,
    pub anvil_cost: i32,
    pub slots: Vec<EquipmentSlotGroup>,
}

impl EnchantmentDefinition {
    /// `EnchantmentDefinition.CODEC` (a `MapCodec`, flattened into the enchantment object).
    pub fn map_codec() -> mc_dfu::MapCodec<EnchantmentDefinition> {
        record!(EnchantmentDefinition {
            supported_items: RegistrySet::<Item>::codec().field_of("supported_items"),
            primary_items: RegistrySet::<Item>::codec().optional_field_of("primary_items"),
            weight: mc_dfu::extra::extra_int_range(1, 1024).field_of("weight"),
            max_level: mc_dfu::extra::extra_int_range(1, 255).field_of("max_level"),
            min_cost: Cost::codec().field_of("min_cost"),
            max_cost: Cost::codec().field_of("max_cost"),
            anvil_cost: mc_dfu::extra::NON_NEGATIVE_INT.field_of("anvil_cost"),
            slots: EquipmentSlotGroup::codec().list_of().field_of("slots"),
        })
    }
}

/// `Enchantment` (the `description`, `definition`, `exclusive_set` and `effects` components).
#[derive(Clone)]
pub struct Enchantment {
    pub description: Component,
    pub definition: EnchantmentDefinition,
    /// `HolderSet<Enchantment>`: a tag or direct references (by registry name).
    pub exclusive_set: HolderSet<NoDirect>,
    pub effects: EnchantmentEffects,
}

impl Enchantment {
    /// `Enchantment.DIRECT_CODEC`.
    pub fn codec() -> Codec<Enchantment> {
        static CODEC: std::sync::LazyLock<Codec<Enchantment>> = std::sync::LazyLock::new(|| {
            record!(Enchantment {
                description: component_codec().field_of("description"),
                definition: EnchantmentDefinition::map_codec(),
                exclusive_set: holder_set_codec::<NoDirect>(ENCHANTMENT_REGISTRY).optional_field_of_default("exclusive_set", HolderSet::Direct(Vec::new())),
                effects: EnchantmentEffects::codec().optional_field_of("effects").xmap(
                    |e: Option<EnchantmentEffects>| e.unwrap_or_default(),
                    |e: &EnchantmentEffects| if e.is_empty() { None } else { Some(e.clone()) },
                ),
            })
            .codec()
        });
        CODEC.clone()
    }

    /// `matchingSlot(slot)`.
    pub fn matching_slot(&self, slot: EquipmentSlot) -> bool {
        self.definition.slots.iter().any(|group| slot_group_test(*group, slot))
    }

    /// `getWeight()`.
    pub fn get_weight(&self) -> i32 {
        self.definition.weight
    }

    /// `getAnvilCost()`.
    pub fn get_anvil_cost(&self) -> i32 {
        self.definition.anvil_cost
    }

    /// `getMinLevel()`.
    pub fn get_min_level(&self) -> i32 {
        1
    }

    /// `getMaxLevel()`.
    pub fn get_max_level(&self) -> i32 {
        self.definition.max_level
    }

    /// `getMinCost(level)`.
    pub fn get_min_cost(&self, level: i32) -> i32 {
        self.definition.min_cost.calculate(level)
    }

    /// `getMaxCost(level)`.
    pub fn get_max_cost(&self, level: i32) -> i32 {
        self.definition.max_cost.calculate(level)
    }

    /// `isSupportedItem(item)`: `item.is(supportedItems)`.
    pub fn is_supported_item(&self, registries: &RegistryAccess, item: &ItemStack) -> bool {
        item_set_contains(registries, &self.definition.supported_items, item.item())
    }

    /// `isPrimaryItem(item)`.
    pub fn is_primary_item(&self, registries: &RegistryAccess, item: &ItemStack) -> bool {
        self.is_supported_item(registries, item)
            && self.definition.primary_items.as_ref().is_none_or(|primary| item_set_contains(registries, primary, item.item()))
    }

    /// `canEnchant(itemStack)`: `supportedItems().contains(itemStack.typeHolder())`.
    pub fn can_enchant(&self, registries: &RegistryAccess, item: &ItemStack) -> bool {
        self.is_supported_item(registries, item)
    }

    /// `getSupportedItems()`.
    pub fn get_supported_items(&self) -> &RegistrySet<Item> {
        &self.definition.supported_items
    }

    /// `exclusiveSet.contains(other)`.
    pub fn exclusive_with(&self, registries: &RegistryAccess, other: &Identifier) -> bool {
        match &self.exclusive_set {
            HolderSet::Named(tag) => registries.is_in_tag(ENCHANTMENT_REGISTRY, other, tag),
            HolderSet::Direct(list) => list.iter().any(|h| h.name() == Some(other)),
        }
    }

    /// `Enchantment.areCompatible(a, b)`.
    pub fn are_compatible(registries: &RegistryAccess, a: &Identifier, b: &Identifier) -> bool {
        if a == b {
            return false;
        }
        let exclusive = |x: &Identifier, y: &Identifier| registries.get::<Enchantment>(ENCHANTMENT_REGISTRY, x).is_some_and(|e| e.exclusive_with(registries, y));
        !exclusive(a, b) && !exclusive(b, a)
    }
}

/// `EquipmentSlotGroup.test(slot)`.
pub fn slot_group_test(group: EquipmentSlotGroup, slot: EquipmentSlot) -> bool {
    match group {
        EquipmentSlotGroup::Any => true,
        EquipmentSlotGroup::MainHand => slot == EquipmentSlot::MainHand,
        EquipmentSlotGroup::OffHand => slot == EquipmentSlot::OffHand,
        EquipmentSlotGroup::Hand => slot.get_type() == EquipmentSlotType::Hand,
        EquipmentSlotGroup::Feet => slot == EquipmentSlot::Feet,
        EquipmentSlotGroup::Legs => slot == EquipmentSlot::Legs,
        EquipmentSlotGroup::Chest => slot == EquipmentSlot::Chest,
        EquipmentSlotGroup::Head => slot == EquipmentSlot::Head,
        EquipmentSlotGroup::Armor => slot.is_armor(),
        EquipmentSlotGroup::Body => slot == EquipmentSlot::Body,
        EquipmentSlotGroup::Saddle => slot == EquipmentSlot::Saddle,
    }
}

/// `item.is(holderSet)` against the tags of `registries` (the level's reloaded tags).
pub fn item_set_contains(registries: &RegistryAccess, set: &RegistrySet<Item>, item: Item) -> bool {
    match set {
        RegistrySet::Entries(items) => items.contains(&item),
        RegistrySet::Tag(tag) => registries
            .registry("minecraft:item")
            .map_or_else(|| set.contains(item), |r| r.is_in_tag(item.id(), tag)),
    }
}
