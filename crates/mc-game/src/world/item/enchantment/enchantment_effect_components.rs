//! `EnchantmentEffectComponents`: the typed `effects` map of an enchantment (a `DataComponentMap` over the
//! `enchantment_effect_component_type` registry), with its codec.

use mc_dfu::{Codec, DataResult, UNIT};
use mc_protocol::components::{Holder, SoundEvent};

use super::codecs::type_codec;
use super::conditional_effect::{ConditionalEffect, TargetedConditionalEffect};
use super::effects::{EnchantmentAttributeEffect, EnchantmentEntityEffect, EnchantmentLocationBasedEffect, EnchantmentValueEffect};

const REGISTRY: &str = "minecraft:enchantment_effect_component_type";

pub type ValueEffects = Vec<ConditionalEffect<EnchantmentValueEffect>>;
pub type EntityEffects = Vec<ConditionalEffect<EnchantmentEntityEffect>>;
pub type DamageImmunities = Vec<ConditionalEffect<()>>;

/// `CrossbowItem.ChargingSounds`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChargingSounds {
    pub start: Option<Holder<SoundEvent>>,
    pub mid: Option<Holder<SoundEvent>>,
    pub end: Option<Holder<SoundEvent>>,
}

impl ChargingSounds {
    pub fn codec() -> Codec<ChargingSounds> {
        mc_dfu::record!(ChargingSounds {
            start: SoundEvent::holder_codec().optional_field_of("start"),
            mid: SoundEvent::holder_codec().optional_field_of("mid"),
            end: SoundEvent::holder_codec().optional_field_of("end"),
        })
        .codec()
    }
}

fn value_list() -> Codec<ValueEffects> {
    ConditionalEffect::codec(&EnchantmentValueEffect::codec()).list_of()
}

fn entity_list() -> Codec<EntityEffects> {
    ConditionalEffect::codec(&EnchantmentEntityEffect::codec()).list_of()
}

/// `MapCodec.unitCodec(DamageImmunity.INSTANCE)` effect: an empty object.
fn damage_immunity() -> Codec<()> {
    UNIT.clone()
}

macro_rules! components {
    ($( $field:ident : $name:literal => $ty:ty = $codec:expr ),* $(,)?) => {
        /// The effect components of an enchantment (`DataComponentMap` of the `enchantment_effect_component_type` registry).
        #[derive(Clone, Default)]
        pub struct EnchantmentEffects {
            $( pub $field: Option<$ty>, )*
        }

        #[allow(non_camel_case_types)]
        #[derive(Clone)]
        enum Value {
            $( $field($ty), )*
        }

        /// The effect component types (`EnchantmentEffectComponents.*`).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[allow(non_camel_case_types)]
        pub enum EffectComponent {
            $( $field, )*
        }

        impl EffectComponent {
            /// Registry name (`minecraft:damage_protection`).
            pub fn name(self) -> &'static str {
                match self { $( EffectComponent::$field => $name, )* }
            }

            pub fn by_name(name: &str) -> Option<EffectComponent> {
                match name { $( $name => Some(EffectComponent::$field), )* _ => None }
            }
        }

        fn value_codec(name: &str) -> Codec<Value> {
            match name {
                $( $name => $codec.xmap(Value::$field, |v: &Value| match v {
                    Value::$field(x) => x.clone(),
                    _ => unreachable!("dispatched by component type"),
                }), )*
                other => unreachable!("not an enchantment effect component type: {other}"),
            }
        }

        impl EnchantmentEffects {
            /// `effects().has(component)`.
            pub fn has(&self, component: EffectComponent) -> bool {
                match component { $( EffectComponent::$field => self.$field.is_some(), )* }
            }

            /// `DataComponentMap.isEmpty()`.
            pub fn is_empty(&self) -> bool {
                true $( && self.$field.is_none() )*
            }

            fn from_entries(entries: Vec<(&'static str, Value)>) -> EnchantmentEffects {
                let mut out = EnchantmentEffects::default();
                for (_, v) in entries {
                    match v { $( Value::$field(x) => out.$field = Some(x), )* }
                }
                out
            }

            fn to_entries(&self) -> Vec<(&'static str, Value)> {
                let mut out = Vec::new();
                $( if let Some(x) = &self.$field { out.push(($name, Value::$field(x.clone()))); } )*
                out
            }
        }
    };
}

components! {
    damage_protection: "minecraft:damage_protection" => ValueEffects = value_list(),
    damage_immunity: "minecraft:damage_immunity" => DamageImmunities = ConditionalEffect::codec(&damage_immunity()).list_of(),
    damage: "minecraft:damage" => ValueEffects = value_list(),
    smash_damage_per_fallen_block: "minecraft:smash_damage_per_fallen_block" => ValueEffects = value_list(),
    knockback: "minecraft:knockback" => ValueEffects = value_list(),
    armor_effectiveness: "minecraft:armor_effectiveness" => ValueEffects = value_list(),
    post_attack: "minecraft:post_attack" => Vec<TargetedConditionalEffect<EnchantmentEntityEffect>> =
        TargetedConditionalEffect::codec(&EnchantmentEntityEffect::codec()).list_of(),
    post_piercing_attack: "minecraft:post_piercing_attack" => EntityEffects = entity_list(),
    hit_block: "minecraft:hit_block" => EntityEffects = entity_list(),
    item_damage: "minecraft:item_damage" => ValueEffects = value_list(),
    equipment_drops: "minecraft:equipment_drops" => Vec<TargetedConditionalEffect<EnchantmentValueEffect>> =
        TargetedConditionalEffect::equipment_drops_codec(&EnchantmentValueEffect::codec()).list_of(),
    location_changed: "minecraft:location_changed" => Vec<ConditionalEffect<EnchantmentLocationBasedEffect>> =
        ConditionalEffect::codec(&EnchantmentLocationBasedEffect::codec()).list_of(),
    tick: "minecraft:tick" => EntityEffects = entity_list(),
    ammo_use: "minecraft:ammo_use" => ValueEffects = value_list(),
    projectile_piercing: "minecraft:projectile_piercing" => ValueEffects = value_list(),
    projectile_spawned: "minecraft:projectile_spawned" => EntityEffects = entity_list(),
    projectile_spread: "minecraft:projectile_spread" => ValueEffects = value_list(),
    projectile_count: "minecraft:projectile_count" => ValueEffects = value_list(),
    trident_return_acceleration: "minecraft:trident_return_acceleration" => ValueEffects = value_list(),
    fishing_time_reduction: "minecraft:fishing_time_reduction" => ValueEffects = value_list(),
    fishing_luck_bonus: "minecraft:fishing_luck_bonus" => ValueEffects = value_list(),
    block_experience: "minecraft:block_experience" => ValueEffects = value_list(),
    mob_experience: "minecraft:mob_experience" => ValueEffects = value_list(),
    repair_with_xp: "minecraft:repair_with_xp" => ValueEffects = value_list(),
    attributes: "minecraft:attributes" => Vec<EnchantmentAttributeEffect> = EnchantmentAttributeEffect::map_codec().codec().list_of(),
    crossbow_charge_time: "minecraft:crossbow_charge_time" => EnchantmentValueEffect = EnchantmentValueEffect::codec(),
    crossbow_charging_sounds: "minecraft:crossbow_charging_sounds" => Vec<ChargingSounds> = ChargingSounds::codec().list_of(),
    trident_sound: "minecraft:trident_sound" => Vec<Holder<SoundEvent>> = SoundEvent::holder_codec().list_of(),
    prevent_equipment_drop: "minecraft:prevent_equipment_drop" => () = UNIT.clone(),
    prevent_armor_change: "minecraft:prevent_armor_change" => () = UNIT.clone(),
    trident_spin_attack_strength: "minecraft:trident_spin_attack_strength" => EnchantmentValueEffect = EnchantmentValueEffect::codec(),
}

impl EnchantmentEffects {
    /// `EnchantmentEffectComponents.CODEC` (`DataComponentMap.makeCodec`).
    pub fn codec() -> Codec<EnchantmentEffects> {
        mc_dfu::extra::dispatched_map(&type_codec(REGISTRY), |name: &&'static str| DataResult::success(value_codec(name))).xmap(
            EnchantmentEffects::from_entries,
            |e: &EnchantmentEffects| e.to_entries(),
        )
    }
}
