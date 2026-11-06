//! `ConditionalEffect` / `TargetedConditionalEffect` / `EnchantmentTarget`.

use mc_dfu::{Codec, record, string_enum_codec};

use super::requirements::{Requirement, requirement_codec};

/// `EnchantmentTarget`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnchantmentTarget {
    Attacker,
    DamagingEntity,
    Victim,
}

static TARGETS: [(EnchantmentTarget, &str); 3] = [
    (EnchantmentTarget::Attacker, "attacker"),
    (EnchantmentTarget::DamagingEntity, "damaging_entity"),
    (EnchantmentTarget::Victim, "victim"),
];
static NON_DAMAGE_TARGETS: [(EnchantmentTarget, &str); 2] = [
    (EnchantmentTarget::Attacker, "attacker"),
    (EnchantmentTarget::Victim, "victim"),
];

impl EnchantmentTarget {
    /// `CODEC`.
    pub fn codec() -> Codec<EnchantmentTarget> {
        string_enum_codec(&TARGETS)
    }

    /// `NON_DAMAGE_CODEC`.
    pub fn non_damage_codec() -> Codec<EnchantmentTarget> {
        string_enum_codec(&NON_DAMAGE_TARGETS)
    }
}

#[derive(Clone)]
pub struct ConditionalEffect<T> {
    pub effect: T,
    pub requirements: Option<Requirement>,
}

#[derive(Clone)]
pub struct TargetedConditionalEffect<T> {
    pub enchanted: EnchantmentTarget,
    pub affected: EnchantmentTarget,
    pub effect: T,
    pub requirements: Option<Requirement>,
}

impl<T: Clone + Send + Sync + 'static> ConditionalEffect<T> {
    /// `ConditionalEffect.codec(effectCodec)`.
    pub fn codec(effect: &Codec<T>) -> Codec<ConditionalEffect<T>> {
        record!(ConditionalEffect<T> {
            effect: effect.field_of("effect"),
            requirements: requirement_codec().optional_field_of("requirements"),
        })
        .codec()
    }
}

impl<T: Clone + Send + Sync + 'static> TargetedConditionalEffect<T> {
    /// `TargetedConditionalEffect.codec(effectCodec)`.
    pub fn codec(effect: &Codec<T>) -> Codec<TargetedConditionalEffect<T>> {
        record!(TargetedConditionalEffect<T> {
            enchanted: EnchantmentTarget::codec().field_of("enchanted"),
            affected: EnchantmentTarget::codec().field_of("affected"),
            effect: effect.field_of("effect"),
            requirements: requirement_codec().optional_field_of("requirements"),
        })
        .codec()
    }

    /// `TargetedConditionalEffect.equipmentDropsCodec(effectCodec)`: `affected` is always the victim.
    pub fn equipment_drops_codec(effect: &Codec<T>) -> Codec<TargetedConditionalEffect<T>> {
        struct Rec<T> {
            enchanted: EnchantmentTarget,
            effect: T,
            requirements: Option<Requirement>,
        }
        record!(Rec<T> {
            enchanted: EnchantmentTarget::non_damage_codec().field_of("enchanted"),
            effect: effect.field_of("effect"),
            requirements: requirement_codec().optional_field_of("requirements"),
        })
        .xmap(
            |r| TargetedConditionalEffect {
                enchanted: r.enchanted,
                affected: EnchantmentTarget::Victim,
                effect: r.effect,
                requirements: r.requirements,
            },
            |t: &TargetedConditionalEffect<T>| Rec {
                enchanted: t.enchanted,
                effect: t.effect.clone(),
                requirements: t.requirements.clone(),
            },
        )
        .codec()
    }
}
