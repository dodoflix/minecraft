//! `FallAfterExplosionTrigger`: `fall_after_explosion`.

use super::prelude::*;
use crate::advancements::predicates::{DistancePredicate, LocationPredicate};

/// `FallAfterExplosionTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct FallAfterExplosionTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub start_position: Option<LocationPredicate>,
    pub distance: Option<DistancePredicate>,
    pub cause: Option<ConditionHolder>,
}

impl TriggerInstance for FallAfterExplosionTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<FallAfterExplosionTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(FallAfterExplosionTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    start_position: LocationPredicate::codec().optional_field_of("start_position"),
                    distance: DistancePredicate::codec().optional_field_of("distance"),
                    cause: condition_holder_codec().optional_field_of("cause"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for FallAfterExplosionTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(FallAfterExplosionTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "cause",
        &this.cause,
    );
});

crate::register_triggers!(FallAfterExplosionTriggerInstance {
    FALL_AFTER_EXPLOSION = "fall_after_explosion",
});

impl FallAfterExplosionTriggerInstance {
    /// `matches(level, enteredPosition, playerPosition, cause)`.
    pub fn matches(
        &self,
        level: &ServerLevel,
        entered_position: Vec3,
        player_position: Vec3,
        cause: Option<&mut LootContext>,
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
        if self.distance.as_ref().is_some_and(|d| {
            !d.matches(
                entered_position.x,
                entered_position.y,
                entered_position.z,
                player_position.x,
                player_position.y,
                player_position.z,
            )
        }) {
            return false;
        }
        match &self.cause {
            None => true,
            Some(_) => cause.is_some_and(|c| holder_matches(&self.cause, c)),
        }
    }
}

impl Trigger<FallAfterExplosionTriggerInstance> {
    /// `trigger(player, startPosition, cause)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        start_position: Vec3,
        cause: Option<&dyn Entity>,
    ) {
        let matches = self.matches(&*level, &*player, start_position, cause);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        start_position: Vec3,
        cause: Option<&dyn Entity>,
    ) -> TriggerMatches {
        let player_position = player.position();
        let cause_params = cause.map(|c| entity_params(level, player_position, c));
        let mut cause_context = cause_params.as_ref().map(new_context);
        self.collect(level, player, |t| {
            t.matches(
                level,
                start_position,
                player_position,
                cause_context.as_mut(),
            )
        })
    }
}
