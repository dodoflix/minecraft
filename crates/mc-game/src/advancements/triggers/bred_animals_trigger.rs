//! `BredAnimalsTrigger`: `bred_animals`.

use super::prelude::*;

/// `BredAnimalsTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct BredAnimalsTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub parent: Option<ConditionHolder>,
    pub partner: Option<ConditionHolder>,
    pub child: Option<ConditionHolder>,
}

impl TriggerInstance for BredAnimalsTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<BredAnimalsTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(BredAnimalsTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                parent: condition_holder_codec().optional_field_of("parent"),
                partner: condition_holder_codec().optional_field_of("partner"),
                child: condition_holder_codec().optional_field_of("child"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for BredAnimalsTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(BredAnimalsTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "parent", &this.parent);
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "partner", &this.partner);
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "child", &this.child);
});

crate::register_triggers!(BredAnimalsTriggerInstance {
    BRED_ANIMALS = "bred_animals",
});

impl BredAnimalsTriggerInstance {
    /// `matches(parent, partner, child)`.
    pub fn matches(&self, parent: &mut LootContext, partner: &mut LootContext, child: Option<&mut LootContext>) -> bool {
        if self.child.is_some() {
            let Some(c) = child else { return false };
            if !holder_matches(&self.child, c) {
                return false;
            }
        }
        (holder_matches(&self.parent, parent) && holder_matches(&self.partner, partner))
            || (holder_matches(&self.parent, partner) && holder_matches(&self.partner, parent))
    }
}

impl Trigger<BredAnimalsTriggerInstance> {
    /// `trigger(player, parent, partner, child)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, parent: &dyn Entity, partner: &dyn Entity, child: Option<&dyn Entity>) {
        let matches = self.matches(&*level, &*player, parent, partner, child);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, parent: &dyn Entity, partner: &dyn Entity, child: Option<&dyn Entity>) -> TriggerMatches {
        let position = player.position();
        let parent_params = entity_params(level, position, parent);
        let partner_params = entity_params(level, position, partner);
        let child_params = child.map(|c| entity_params(level, position, c));
        let mut parent_context = new_context(&parent_params);
        let mut partner_context = new_context(&partner_params);
        let mut child_context = child_params.as_ref().map(new_context);
        self.collect(level, player, |t| t.matches(&mut parent_context, &mut partner_context, child_context.as_mut()))
    }
}
