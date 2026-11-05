//! `ChangeDimensionTrigger`: `changed_dimension`.

use super::prelude::*;
use mc_core::ident::Identifier;
use mc_dfu::extra::IDENTIFIER;

/// `ChangeDimensionTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct ChangeDimensionTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub from: Option<Identifier>,
    pub to: Option<Identifier>,
}

impl TriggerInstance for ChangeDimensionTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<ChangeDimensionTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(ChangeDimensionTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                from: IDENTIFIER.optional_field_of("from"),
                to: IDENTIFIER.optional_field_of("to"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for ChangeDimensionTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(ChangeDimensionTriggerInstance);

crate::register_triggers!(ChangeDimensionTriggerInstance {
    CHANGED_DIMENSION = "changed_dimension",
});

impl ChangeDimensionTriggerInstance {
    /// `matches(from, to)`.
    pub fn matches(&self, from: &Identifier, to: &Identifier) -> bool {
        if self.from.as_ref().is_some_and(|f| f != from) {
            return false;
        }
        self.to.as_ref().is_none_or(|t| t == to)
    }
}

impl Trigger<ChangeDimensionTriggerInstance> {
    /// `trigger(player, from, to)` (dimension keys).
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, from: &Identifier, to: &Identifier) {
        self.trigger_with(level, player, |t| t.matches(from, to));
    }
}
