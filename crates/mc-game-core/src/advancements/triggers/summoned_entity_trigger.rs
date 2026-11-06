//! `SummonedEntityTrigger`: `summoned_entity`.

use super::prelude::*;

/// `SummonedEntityTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct SummonedEntityTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub entity: Option<ConditionHolder>,
}

impl TriggerInstance for SummonedEntityTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<SummonedEntityTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(SummonedEntityTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    entity: condition_holder_codec().optional_field_of("entity"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for SummonedEntityTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(SummonedEntityTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "entity",
        &this.entity,
    );
});

crate::register_triggers!(SummonedEntityTriggerInstance {
    SUMMONED_ENTITY = "summoned_entity",
});

impl SummonedEntityTriggerInstance {
    /// `matches(entityContext)`.
    pub fn matches(&self, context: &mut LootContext) -> bool {
        holder_matches(&self.entity, context)
    }
}

impl Trigger<SummonedEntityTriggerInstance> {
    /// `trigger(player, entity)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, entity: &dyn Entity) {
        let matches = self.matches(&*level, &*player, entity);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        entity: &dyn Entity,
    ) -> TriggerMatches {
        let params = entity_params(level, player.position(), entity);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(&mut context))
    }
}
