//! `LevitationTrigger`: `levitation`.

use super::prelude::*;
use crate::advancements::predicates::DistancePredicate;

/// `LevitationTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct LevitationTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub distance: Option<DistancePredicate>,
    pub duration: Ints,
}

impl TriggerInstance for LevitationTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<LevitationTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(LevitationTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    distance: DistancePredicate::codec().optional_field_of("distance"),
                    duration: Ints::codec().optional_field_of_default("duration", Ints::any()),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for LevitationTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(LevitationTriggerInstance);

crate::register_triggers!(LevitationTriggerInstance {
    LEVITATION = "levitation",
});

impl LevitationTriggerInstance {
    /// `matches(player, start, duration)`.
    pub fn matches(&self, player: &ServerPlayer, start: Vec3, duration: i32) -> bool {
        if let Some(distance) = &self.distance {
            let p = player.position();
            if !distance.matches(start.x, start.y, start.z, p.x, p.y, p.z) {
                return false;
            }
        }
        self.duration.matches(duration)
    }
}

impl Trigger<LevitationTriggerInstance> {
    /// `trigger(player, start, duration)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        start: Vec3,
        duration: i32,
    ) {
        let matches = self.collect(&*level, &*player, |t| t.matches(player, start, duration));
        matches.award(level, player);
    }
}
