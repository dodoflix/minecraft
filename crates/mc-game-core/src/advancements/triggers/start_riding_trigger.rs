//! `StartRidingTrigger`: `started_riding`.

use super::prelude::*;

/// `StartRidingTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct StartRidingTriggerInstance {
    pub player: Option<ConditionHolder>,
}

impl TriggerInstance for StartRidingTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<StartRidingTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(StartRidingTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for StartRidingTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(StartRidingTriggerInstance);

crate::register_triggers!(StartRidingTriggerInstance {
    START_RIDING_TRIGGER = "started_riding",
});

impl Trigger<StartRidingTriggerInstance> {
    /// `trigger(player)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer) {
        self.trigger_with(level, player, |_| true);
    }
}
