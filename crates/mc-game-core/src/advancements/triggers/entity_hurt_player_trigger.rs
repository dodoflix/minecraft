//! `EntityHurtPlayerTrigger`: `entity_hurt_player`.

use super::prelude::*;
use crate::advancements::predicates::DamagePredicate;

/// `EntityHurtPlayerTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct EntityHurtPlayerTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub damage: Option<DamagePredicate>,
}

impl TriggerInstance for EntityHurtPlayerTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<EntityHurtPlayerTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(EntityHurtPlayerTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    damage: DamagePredicate::codec().optional_field_of("damage"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for EntityHurtPlayerTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(EntityHurtPlayerTriggerInstance);

crate::register_triggers!(EntityHurtPlayerTriggerInstance {
    ENTITY_HURT_PLAYER = "entity_hurt_player",
});

impl EntityHurtPlayerTriggerInstance {
    /// `matches(player, source, originalDamage, actualDamage, blocked)`.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        source: &DamageSource,
        original_damage: f32,
        actual_damage: f32,
        blocked: bool,
    ) -> bool {
        self.damage.as_ref().is_none_or(|d| {
            d.matches(
                level,
                player,
                source,
                original_damage,
                actual_damage,
                blocked,
            )
        })
    }
}

impl Trigger<EntityHurtPlayerTriggerInstance> {
    /// `trigger(player, source, originalDamage, actualDamage, blocked)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        source: &DamageSource,
        original_damage: f32,
        actual_damage: f32,
        blocked: bool,
    ) {
        let matches = self.matches(
            &*level,
            &*player,
            source,
            original_damage,
            actual_damage,
            blocked,
        );
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        source: &DamageSource,
        original_damage: f32,
        actual_damage: f32,
        blocked: bool,
    ) -> TriggerMatches {
        self.collect(level, player, |t| {
            t.matches(
                level,
                player,
                source,
                original_damage,
                actual_damage,
                blocked,
            )
        })
    }
}
