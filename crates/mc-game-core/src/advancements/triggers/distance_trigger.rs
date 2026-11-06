//! `DistanceTrigger`: `nether_travel`, `fall_from_height`, `ride_entity_in_lava`.

use super::prelude::*;
use crate::advancements::predicates::{DistancePredicate, LocationPredicate};

/// `DistanceTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct DistanceTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub start_position: Option<LocationPredicate>,
    pub distance: Option<DistancePredicate>,
}

impl TriggerInstance for DistanceTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<DistanceTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(DistanceTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    start_position: LocationPredicate::codec().optional_field_of("start_position"),
                    distance: DistancePredicate::codec().optional_field_of("distance"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for DistanceTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(DistanceTriggerInstance);

crate::register_triggers!(DistanceTriggerInstance {
    NETHER_TRAVEL = "nether_travel",
    FALL_FROM_HEIGHT = "fall_from_height",
    RIDE_ENTITY_IN_LAVA_TRIGGER = "ride_entity_in_lava",
});

impl DistanceTriggerInstance {
    /// `matches(level, enteredPosition, playerPosition)`.
    pub fn matches(
        &self,
        level: &ServerLevel,
        entered_position: Vec3,
        player_position: Vec3,
    ) -> bool {
        if self.start_position.as_ref().is_some_and(|p| {
            !p.matches(
                level,
                entered_position.x,
                entered_position.y,
                entered_position.z,
            )
        }) {
            return false;
        }
        self.distance.as_ref().is_none_or(|d| {
            d.matches(
                entered_position.x,
                entered_position.y,
                entered_position.z,
                player_position.x,
                player_position.y,
                player_position.z,
            )
        })
    }
}

impl Trigger<DistanceTriggerInstance> {
    /// `trigger(player, startPosition)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        start_position: Vec3,
    ) {
        let player_position = player.position();
        let matches = self.collect(&*level, &*player, |t| {
            t.matches(level, start_position, player_position)
        });
        matches.award(level, player);
    }
}
