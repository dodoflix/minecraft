//! The mob-effect parts of `Consumable.onConsume`: the `ConsumableListener`s of potion contents, suspicious stew
//! effects and the ominous bottle, and the status-effect `ConsumeEffect`s.

use super::potion_contents;
use crate::server::level::ServerLevel;
use crate::world::entity::Entity;
use crate::world::entity::effect::MobEffectInstance;
use crate::world::item::{ItemStack, ItemStackExt};
use mc_core::random::RandomSource;
use mc_data::MobEffect;
use mc_protocol::components::{
    ApplyStatusEffectsConsumeEffect, OMINOUS_BOTTLE_AMPLIFIER, POTION_CONTENTS, POTION_DURATION_SCALE, RegistrySet, RemoveStatusEffectsConsumeEffect, SUSPICIOUS_STEW_EFFECTS,
};

/// `stack.getAllOfType(ConsumableListener.class).forEach(listener -> listener.onConsume(level, user, stack, consumable))`
/// for the effect-applying listeners (`FoodProperties` is handled by the caller).
pub fn on_consume_listeners(level: &mut ServerLevel, user: &mut dyn Entity, stack: &ItemStack) {
    if let Some(contents) = stack.get(&POTION_CONTENTS) {
        // `PotionContents.onConsume`
        let scale = stack.get(&POTION_DURATION_SCALE).copied().unwrap_or(1.0);
        potion_contents::apply_to_living_entity(contents, level, user, scale);
    }
    if let Some(stew) = stack.get(&SUSPICIOUS_STEW_EFFECTS) {
        // `SuspiciousStewEffects.onConsume`: `new MobEffectInstance(effect, duration)`
        for entry in &stew.effects {
            user.add_effect(level, MobEffectInstance::of(entry.effect, entry.duration, 0), None);
        }
    }
    if let Some(amplifier) = stack.get(&OMINOUS_BOTTLE_AMPLIFIER) {
        // `OminousBottleAmplifier.onConsume`
        user.add_effect(level, MobEffectInstance::new(MobEffect::BAD_OMEN, 120000, amplifier.value, false, false, true, None), None);
    }
}

/// `ApplyStatusEffectsConsumeEffect.apply`.
pub fn apply_status_effects(effect: &ApplyStatusEffectsConsumeEffect, level: &mut ServerLevel, user: &mut dyn Entity) -> bool {
    if user.base_mut().random.next_float() >= effect.probability {
        return false;
    }
    let mut any_applied = false;
    for e in &effect.effects {
        if user.add_effect(level, MobEffectInstance::from(e), None) {
            any_applied = true;
        }
    }
    any_applied
}

/// `RemoveStatusEffectsConsumeEffect.apply`.
pub fn remove_status_effects(effect: &RemoveStatusEffectsConsumeEffect, level: &mut ServerLevel, user: &mut dyn Entity) -> bool {
    // `for (Holder<MobEffect> effect : this.effects)`: list order, or the tag's order
    let effects: Vec<MobEffect> = match &effect.effects {
        RegistrySet::Entries(v) => v.clone(),
        RegistrySet::Tag(t) => mc_data::tags::get::<MobEffect>(&t.to_string()).map(|s| s.iter().collect()).unwrap_or_default(),
    };
    let mut has_removed_any = false;
    for e in effects {
        if user.remove_effect(level, e) {
            has_removed_any = true;
        }
    }
    has_removed_any
}
