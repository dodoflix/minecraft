//! The `DataComponentPredicate` implementations (`net.minecraft.core.component.predicates.*`) and the item-side
//! predicates they are built from (`EnchantmentPredicate`, `MobEffectsPredicate`, `ItemPredicate`).
//!
//! Only the codecs live here; the persistent (NBT) codec is also the stream codec of every predicate
//! (`ByteBufCodecs.fromCodecWithRegistries`).

use super::bounds::{Doubles, Ints};
use super::collection::CollectionPredicate;
use super::matchers::DataComponentMatchers;
use super::state::NbtPredicate;
use crate::components::common::{component_codec, RegistrySet};
use crate::components::holder::{holder_fixed_codec, holder_set_codec, registry_by_name_codec, HolderSet, NoDirect};
use crate::packets::play::AttributeModifierOperation;
use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;
use mc_dfu::{string_enum_codec, Codec, BOOL, STRING};
use mc_text::Component;
use std::sync::LazyLock;

/// `RegistryCodecs.holderSet(key)` of a data-driven registry (entries are only referenced, never inline).
pub fn data_driven_set_codec(registry: &'static str) -> Codec<HolderSet<NoDirect>> {
    holder_set_codec(registry, &holder_fixed_codec::<NoDirect>(registry), false)
}

// ---- damage -------------------------------------------------------------------------------------------------------

/// `DamagePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct DamagePredicate {
    pub durability: Ints,
    pub damage: Ints,
}

impl DamagePredicate {
    pub fn codec() -> Codec<DamagePredicate> {
        mc_dfu::record!(DamagePredicate {
            durability: Ints::codec().optional_field_of_default("durability", Ints::any()),
            damage: Ints::codec().optional_field_of_default("damage", Ints::any()),
        })
        .codec()
    }
}

// ---- enchantments -------------------------------------------------------------------------------------------------

/// `EnchantmentPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentPredicate {
    pub enchantments: Option<HolderSet<NoDirect>>,
    pub level: Ints,
}

impl EnchantmentPredicate {
    pub fn codec() -> Codec<EnchantmentPredicate> {
        static C: LazyLock<Codec<EnchantmentPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(EnchantmentPredicate {
                enchantments: data_driven_set_codec("minecraft:enchantment").optional_field_of("enchantments"),
                level: Ints::codec().optional_field_of_default("levels", Ints::any()),
            })
            .codec()
        });
        C.clone()
    }
}

/// `EnchantmentsPredicate` (`Enchantments` and `StoredEnchantments` share the content).
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentsPredicate {
    pub enchantments: Vec<EnchantmentPredicate>,
}

impl EnchantmentsPredicate {
    pub fn codec() -> Codec<EnchantmentsPredicate> {
        EnchantmentPredicate::codec().list_of().xmap(|enchantments| EnchantmentsPredicate { enchantments }, |p: &EnchantmentsPredicate| {
            p.enchantments.clone()
        })
    }
}

// ---- potions / mob effects ----------------------------------------------------------------------------------------

/// `MobEffectsPredicate.MobEffectInstancePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct MobEffectInstancePredicate {
    pub amplifier: Ints,
    pub duration: Ints,
    pub ambient: Option<bool>,
    pub visible: Option<bool>,
}

impl MobEffectInstancePredicate {
    pub fn codec() -> Codec<MobEffectInstancePredicate> {
        mc_dfu::record!(MobEffectInstancePredicate {
            amplifier: Ints::codec().optional_field_of_default("amplifier", Ints::any()),
            duration: Ints::codec().optional_field_of_default("duration", Ints::any()),
            ambient: BOOL.optional_field_of("ambient"),
            visible: BOOL.optional_field_of("visible"),
        })
        .codec()
    }
}

/// `MobEffectsPredicate`: effect -> instance predicate. Map equality ignores the order.
#[derive(Clone, Debug)]
pub struct MobEffectsPredicate {
    pub effect_map: Vec<(mc_data::MobEffect, MobEffectInstancePredicate)>,
}

impl PartialEq for MobEffectsPredicate {
    fn eq(&self, other: &Self) -> bool {
        self.effect_map.len() == other.effect_map.len()
            && self.effect_map.iter().all(|(k, v)| other.effect_map.iter().any(|(k2, v2)| k == k2 && v == v2))
    }
}

impl MobEffectsPredicate {
    pub fn codec() -> Codec<MobEffectsPredicate> {
        registry_by_name_codec::<mc_data::MobEffect>()
            .unbounded_map(&MobEffectInstancePredicate::codec())
            .xmap(|effect_map| MobEffectsPredicate { effect_map }, |p: &MobEffectsPredicate| p.effect_map.clone())
    }
}

/// `PotionsPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct PotionsPredicate {
    pub potions: Option<RegistrySet<mc_data::Potion>>,
    pub effects: Option<CollectionPredicate<MobEffectsPredicate>>,
}

impl PotionsPredicate {
    pub fn codec() -> Codec<PotionsPredicate> {
        mc_dfu::record!(PotionsPredicate {
            potions: RegistrySet::<mc_data::Potion>::codec().optional_field_of("potions"),
            effects: CollectionPredicate::codec(&MobEffectsPredicate::codec()).optional_field_of("effects"),
        })
        .codec()
    }
}

// ---- custom data --------------------------------------------------------------------------------------------------

/// `CustomDataPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomDataPredicate {
    pub value: NbtPredicate,
}

impl CustomDataPredicate {
    pub fn codec() -> Codec<CustomDataPredicate> {
        NbtPredicate::codec().xmap(|value| CustomDataPredicate { value }, |p: &CustomDataPredicate| p.value.clone())
    }
}

// ---- container / bundle -------------------------------------------------------------------------------------------

/// `ContainerPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerPredicate {
    pub items: Option<CollectionPredicate<ItemPredicate>>,
}

impl ContainerPredicate {
    pub fn codec() -> Codec<ContainerPredicate> {
        mc_dfu::record!(ContainerPredicate { items: CollectionPredicate::codec(&ItemPredicate::codec()).optional_field_of("items") }).codec()
    }
}

/// `BundlePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct BundlePredicate {
    pub items: Option<CollectionPredicate<ItemPredicate>>,
}

impl BundlePredicate {
    pub fn codec() -> Codec<BundlePredicate> {
        mc_dfu::record!(BundlePredicate { items: CollectionPredicate::codec(&ItemPredicate::codec()).optional_field_of("items") }).codec()
    }
}

/// `ItemPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemPredicate {
    pub items: Option<RegistrySet<mc_data::Item>>,
    pub count: Ints,
    pub components: DataComponentMatchers,
}

impl ItemPredicate {
    /// `ItemPredicate.CODEC`.
    pub fn codec() -> Codec<ItemPredicate> {
        static C: LazyLock<Codec<ItemPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(ItemPredicate {
                items: RegistrySet::<mc_data::Item>::codec().optional_field_of("items"),
                count: Ints::codec().optional_field_of_default("count", Ints::any()),
                components: DataComponentMatchers::map_codec(),
            })
            .codec()
        });
        C.clone()
    }
}

// ---- fireworks ----------------------------------------------------------------------------------------------------

crate::string_enum! {
    /// `FireworkExplosion.Shape`.
    pub enum FireworkShape { SmallBall = "small_ball", LargeBall = "large_ball", Star = "star", Creeper = "creeper", Burst = "burst" }
}

/// `FireworkExplosionPredicate.FireworkPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct FireworkPredicate {
    pub shape: Option<FireworkShape>,
    pub twinkle: Option<bool>,
    pub trail: Option<bool>,
}

impl FireworkPredicate {
    pub fn codec() -> Codec<FireworkPredicate> {
        mc_dfu::record!(FireworkPredicate {
            shape: FireworkShape::codec().optional_field_of("shape"),
            twinkle: BOOL.optional_field_of("has_twinkle"),
            trail: BOOL.optional_field_of("has_trail"),
        })
        .codec()
    }
}

/// `FireworkExplosionPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct FireworkExplosionPredicate {
    pub predicate: FireworkPredicate,
}

impl FireworkExplosionPredicate {
    pub fn codec() -> Codec<FireworkExplosionPredicate> {
        FireworkPredicate::codec().xmap(|predicate| FireworkExplosionPredicate { predicate }, |p: &FireworkExplosionPredicate| {
            p.predicate.clone()
        })
    }
}

/// `FireworksPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct FireworksPredicate {
    pub explosions: Option<CollectionPredicate<FireworkPredicate>>,
    pub flight_duration: Ints,
}

impl FireworksPredicate {
    pub fn codec() -> Codec<FireworksPredicate> {
        mc_dfu::record!(FireworksPredicate {
            explosions: CollectionPredicate::codec(&FireworkPredicate::codec()).optional_field_of("explosions"),
            flight_duration: Ints::codec().optional_field_of_default("flight_duration", Ints::any()),
        })
        .codec()
    }
}

// ---- books --------------------------------------------------------------------------------------------------------

/// `WritableBookPredicate.PagePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct WritableBookPagePredicate {
    pub contents: String,
}

/// `WritableBookPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct WritableBookPredicate {
    pub pages: Option<CollectionPredicate<WritableBookPagePredicate>>,
}

impl WritableBookPredicate {
    pub fn codec() -> Codec<WritableBookPredicate> {
        let page = STRING.xmap(|contents| WritableBookPagePredicate { contents }, |p: &WritableBookPagePredicate| p.contents.clone());
        mc_dfu::record!(WritableBookPredicate { pages: CollectionPredicate::codec(&page).optional_field_of("pages") }).codec()
    }
}

/// `WrittenBookPredicate.PagePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct WrittenBookPagePredicate {
    pub contents: Component,
}

/// `WrittenBookPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct WrittenBookPredicate {
    pub pages: Option<CollectionPredicate<WrittenBookPagePredicate>>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub generation: Ints,
    pub resolved: Option<bool>,
}

impl WrittenBookPredicate {
    pub fn codec() -> Codec<WrittenBookPredicate> {
        let page = component_codec().xmap(|contents| WrittenBookPagePredicate { contents }, |p: &WrittenBookPagePredicate| p.contents.clone());
        mc_dfu::record!(WrittenBookPredicate {
            pages: CollectionPredicate::codec(&page).optional_field_of("pages"),
            author: STRING.optional_field_of("author"),
            title: STRING.optional_field_of("title"),
            generation: Ints::codec().optional_field_of_default("generation", Ints::any()),
            resolved: BOOL.optional_field_of("resolved"),
        })
        .codec()
    }
}

// ---- attribute modifiers ------------------------------------------------------------------------------------------

crate::string_enum! {
    /// `EquipmentSlotGroup`.
    pub enum SlotGroup {
        Any = "any", MainHand = "mainhand", OffHand = "offhand", Hand = "hand", Feet = "feet", Legs = "legs",
        Chest = "chest", Head = "head", Armor = "armor", Body = "body", Saddle = "saddle",
    }
}

static OPERATIONS: [(AttributeModifierOperation, &str); 3] = [
    (AttributeModifierOperation::AddValue, "add_value"),
    (AttributeModifierOperation::AddMultipliedBase, "add_multiplied_base"),
    (AttributeModifierOperation::AddMultipliedTotal, "add_multiplied_total"),
];

/// `AttributeModifiersPredicate.EntryPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct AttributeEntryPredicate {
    pub attribute: Option<RegistrySet<mc_data::Attribute>>,
    pub id: Option<Identifier>,
    pub amount: Doubles,
    pub operation: Option<AttributeModifierOperation>,
    pub slot: Option<SlotGroup>,
}

impl AttributeEntryPredicate {
    pub fn codec() -> Codec<AttributeEntryPredicate> {
        mc_dfu::record!(AttributeEntryPredicate {
            attribute: RegistrySet::<mc_data::Attribute>::codec().optional_field_of("attribute"),
            id: IDENTIFIER.optional_field_of("id"),
            amount: Doubles::codec().optional_field_of_default("amount", Doubles::any()),
            operation: string_enum_codec(&OPERATIONS).optional_field_of("operation"),
            slot: SlotGroup::codec().optional_field_of("slot"),
        })
        .codec()
    }
}

/// `AttributeModifiersPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct AttributeModifiersPredicate {
    pub modifiers: Option<CollectionPredicate<AttributeEntryPredicate>>,
}

impl AttributeModifiersPredicate {
    pub fn codec() -> Codec<AttributeModifiersPredicate> {
        mc_dfu::record!(AttributeModifiersPredicate {
            modifiers: CollectionPredicate::codec(&AttributeEntryPredicate::codec()).optional_field_of("modifiers"),
        })
        .codec()
    }
}

// ---- trim / jukebox / villager ------------------------------------------------------------------------------------

/// `TrimPredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrimPredicate {
    pub material: Option<HolderSet<NoDirect>>,
    pub pattern: Option<HolderSet<NoDirect>>,
}

impl TrimPredicate {
    pub fn codec() -> Codec<TrimPredicate> {
        mc_dfu::record!(TrimPredicate {
            material: data_driven_set_codec("minecraft:trim_material").optional_field_of("material"),
            pattern: data_driven_set_codec("minecraft:trim_pattern").optional_field_of("pattern"),
        })
        .codec()
    }
}

/// `JukeboxPlayablePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct JukeboxPlayablePredicate {
    pub song: Option<HolderSet<NoDirect>>,
}

impl JukeboxPlayablePredicate {
    pub fn codec() -> Codec<JukeboxPlayablePredicate> {
        mc_dfu::record!(JukeboxPlayablePredicate { song: data_driven_set_codec("minecraft:jukebox_song").optional_field_of("song") }).codec()
    }
}

/// `VillagerTypePredicate`.
#[derive(Clone, Debug, PartialEq)]
pub struct VillagerTypePredicate {
    pub villager_types: RegistrySet<mc_data::VillagerType>,
}

impl VillagerTypePredicate {
    pub fn codec() -> Codec<VillagerTypePredicate> {
        RegistrySet::<mc_data::VillagerType>::codec()
            .xmap(|villager_types| VillagerTypePredicate { villager_types }, |p: &VillagerTypePredicate| p.villager_types.clone())
    }
}
