//! `TameAnimalTrigger`: `tame_animal`.

use super::prelude::*;

/// `TameAnimalTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct TameAnimalTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub entity: Option<ConditionHolder>,
}

impl TriggerInstance for TameAnimalTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<TameAnimalTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(TameAnimalTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                entity: condition_holder_codec().optional_field_of("entity"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for TameAnimalTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(TameAnimalTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "entity", &this.entity);
});

crate::register_triggers!(TameAnimalTriggerInstance {
    TAME_ANIMAL = "tame_animal",
});

impl TameAnimalTriggerInstance {
    /// `matches(animalContext)`.
    pub fn matches(&self, context: &mut LootContext) -> bool {
        holder_matches(&self.entity, context)
    }
}

impl Trigger<TameAnimalTriggerInstance> {
    /// `trigger(player, animal)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, animal: &dyn Entity) {
        let matches = self.matches(&*level, &*player, animal);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, animal: &dyn Entity) -> TriggerMatches {
        let params = entity_params(level, player.position(), animal);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(&mut context))
    }
}
