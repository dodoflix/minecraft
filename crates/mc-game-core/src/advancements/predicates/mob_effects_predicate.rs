//! `MobEffectsPredicate.matches(entity)` (codec and `MobEffectInstancePredicate` live in mc-protocol).

use crate::world::entity::Entity;
use crate::world::entity::effect::MobEffectInstance;
use indexmap::IndexMap;
use mc_data::MobEffect;
use mc_protocol::components::{MobEffectInstancePredicate, MobEffectsPredicate};

pub trait MobEffectsPredicateEntityExt {
    /// `matches(Entity)`: only living entities have effects.
    fn matches_entity(&self, entity: &dyn Entity) -> bool;
    /// `matches(Map<Holder<MobEffect>, MobEffectInstance>)`.
    fn matches_effects(&self, effects: &IndexMap<MobEffect, MobEffectInstance>) -> bool;
}

/// `MobEffectInstancePredicate.matches(@Nullable MobEffectInstance)`.
pub fn instance_matches(
    predicate: &MobEffectInstancePredicate,
    instance: Option<&MobEffectInstance>,
) -> bool {
    let Some(instance) = instance else {
        return false;
    };
    predicate.amplifier.matches(instance.amplifier)
        && predicate.duration.matches(instance.duration)
        && predicate.ambient.is_none_or(|a| a == instance.ambient)
        && predicate.visible.is_none_or(|v| v == instance.visible)
}

impl MobEffectsPredicateEntityExt for MobEffectsPredicate {
    fn matches_entity(&self, entity: &dyn Entity) -> bool {
        entity
            .living()
            .is_some_and(|living| self.matches_effects(&living.active_effects))
    }

    fn matches_effects(&self, effects: &IndexMap<MobEffect, MobEffectInstance>) -> bool {
        self.effect_map
            .iter()
            .all(|(effect, predicate)| instance_matches(predicate, effects.get(effect)))
    }
}
