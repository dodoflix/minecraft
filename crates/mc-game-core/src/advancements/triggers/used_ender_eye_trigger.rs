//! `UsedEnderEyeTrigger`: `used_ender_eye`.

use super::prelude::*;

/// `UsedEnderEyeTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct UsedEnderEyeTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub distance: Doubles,
}

impl TriggerInstance for UsedEnderEyeTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<UsedEnderEyeTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(UsedEnderEyeTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    distance: Doubles::codec()
                        .optional_field_of_default("distance", Doubles::any()),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for UsedEnderEyeTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(UsedEnderEyeTriggerInstance);

crate::register_triggers!(UsedEnderEyeTriggerInstance {
    USED_ENDER_EYE = "used_ender_eye",
});

impl UsedEnderEyeTriggerInstance {
    /// `matches(sqrDistance)`.
    pub fn matches(&self, sqr_distance: f64) -> bool {
        self.distance.matches_sqr(sqr_distance)
    }
}

impl Trigger<UsedEnderEyeTriggerInstance> {
    /// `trigger(player, feature)`: the horizontal distance to the stronghold position.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, feature: BlockPos) {
        let xd = player.base().x() - f64::from(feature.x());
        let zd = player.base().z() - f64::from(feature.z());
        let sqr_dist = xd * xd + zd * zd;
        self.trigger_with(level, player, |t| t.matches(sqr_dist));
    }
}
