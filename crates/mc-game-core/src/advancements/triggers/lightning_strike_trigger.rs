//! `LightningStrikeTrigger`: `lightning_strike`.

use super::prelude::*;

/// `LightningStrikeTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct LightningStrikeTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub lightning: Option<ConditionHolder>,
    pub bystander: Option<ConditionHolder>,
}

impl TriggerInstance for LightningStrikeTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<LightningStrikeTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(LightningStrikeTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    lightning: condition_holder_codec().optional_field_of("lightning"),
                    bystander: condition_holder_codec().optional_field_of("bystander"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for LightningStrikeTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(LightningStrikeTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "lightning",
        &this.lightning,
    );
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "bystander",
        &this.bystander,
    );
});

crate::register_triggers!(LightningStrikeTriggerInstance {
    LIGHTNING_STRIKE = "lightning_strike",
});

impl LightningStrikeTriggerInstance {
    /// `matches(bolt, entitiesAround)`.
    pub fn matches(&self, bolt: &mut LootContext, entities_around: &mut [LootContext]) -> bool {
        if self.lightning.is_some() && !holder_matches(&self.lightning, bolt) {
            return false;
        }
        match &self.bystander {
            None => true,
            Some(_) => entities_around
                .iter_mut()
                .any(|c| holder_matches(&self.bystander, c)),
        }
    }
}

impl Trigger<LightningStrikeTriggerInstance> {
    /// `trigger(player, lightning, entitiesAround)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        lightning: &dyn Entity,
        entities_around: &[&dyn Entity],
    ) {
        let matches = self.matches(&*level, &*player, lightning, entities_around);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        lightning: &dyn Entity,
        entities_around: &[&dyn Entity],
    ) -> TriggerMatches {
        let position = player.position();
        let around_params: Vec<_> = entities_around
            .iter()
            .map(|e| entity_params(level, position, *e))
            .collect();
        let mut around: Vec<LootContext> = around_params.iter().map(new_context).collect();
        let lightning_params = entity_params(level, position, lightning);
        let mut lightning_context = new_context(&lightning_params);
        self.collect(level, player, |t| {
            t.matches(&mut lightning_context, &mut around)
        })
    }
}
