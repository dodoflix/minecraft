//! `SpearMobsTrigger`: `spear_mobs`.

use super::prelude::*;

/// `SpearMobsTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct SpearMobsTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub count: Option<i32>,
}

impl TriggerInstance for SpearMobsTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<SpearMobsTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(SpearMobsTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                count: mc_dfu::extra::POSITIVE_INT.optional_field_of("count"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for SpearMobsTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(SpearMobsTriggerInstance);

crate::register_triggers!(SpearMobsTriggerInstance {
    SPEAR_MOBS_TRIGGER = "spear_mobs",
});

impl SpearMobsTriggerInstance {
    /// `matches(requiredCount)`.
    pub fn matches(&self, required_count: i32) -> bool {
        self.count.is_none_or(|c| required_count >= c)
    }
}

impl Trigger<SpearMobsTriggerInstance> {
    /// `trigger(player, number)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, number: i32) {
        self.trigger_with(level, player, |t| t.matches(number));
    }
}
