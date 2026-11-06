//! `ConstructBeaconTrigger`: `construct_beacon`.

use super::prelude::*;

/// `ConstructBeaconTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct ConstructBeaconTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub level: Ints,
}

impl TriggerInstance for ConstructBeaconTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<ConstructBeaconTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(ConstructBeaconTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    level: Ints::codec().optional_field_of_default("level", Ints::any()),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for ConstructBeaconTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(ConstructBeaconTriggerInstance);

crate::register_triggers!(ConstructBeaconTriggerInstance {
    CONSTRUCT_BEACON = "construct_beacon",
});

impl ConstructBeaconTriggerInstance {
    /// `matches(levels)`.
    pub fn matches(&self, levels: i32) -> bool {
        self.level.matches(levels)
    }
}

impl Trigger<ConstructBeaconTriggerInstance> {
    /// `trigger(player, levels)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, levels: i32) {
        self.trigger_with(level, player, |t| t.matches(levels));
    }
}
