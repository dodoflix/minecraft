//! `EffectsChangedTrigger`: `effects_changed`.

use super::prelude::*;
use mc_protocol::components::MobEffectsPredicate;
use crate::advancements::predicates::MobEffectsPredicateEntityExt;

/// `EffectsChangedTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct EffectsChangedTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub effects: Option<MobEffectsPredicate>,
    pub source: Option<ConditionHolder>,
}

impl TriggerInstance for EffectsChangedTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<EffectsChangedTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(EffectsChangedTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                effects: MobEffectsPredicate::codec().optional_field_of("effects"),
                source: condition_holder_codec().optional_field_of("source"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for EffectsChangedTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(EffectsChangedTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "source", &this.source);
});

crate::register_triggers!(EffectsChangedTriggerInstance {
    EFFECTS_CHANGED = "effects_changed",
});

impl EffectsChangedTriggerInstance {
    /// `matches(player, source)`.
    pub fn matches(&self, player: &ServerPlayer, source: Option<&mut LootContext>) -> bool {
        if self.effects.as_ref().is_some_and(|e| !e.matches_entity(player)) {
            return false;
        }
        match &self.source {
            None => true,
            Some(_) => source.is_some_and(|c| holder_matches(&self.source, c)),
        }
    }
}

impl Trigger<EffectsChangedTriggerInstance> {
    /// `trigger(player, source)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, source: Option<&dyn Entity>) {
        let matches = self.matches(&*level, &*player, source);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, source: Option<&dyn Entity>) -> TriggerMatches {
        let params = source.map(|s| entity_params(level, player.position(), s));
        let mut context = params.as_ref().map(new_context);
        self.collect(level, player, |t| t.matches(player, context.as_mut()))
    }
}
