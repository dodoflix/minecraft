//! `PlayerHurtEntityTrigger`: `player_hurt_entity`.

use super::prelude::*;
use crate::advancements::predicates::DamagePredicate;

/// `PlayerHurtEntityTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct PlayerHurtEntityTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub damage: Option<DamagePredicate>,
    pub entity: Option<ConditionHolder>,
}

impl TriggerInstance for PlayerHurtEntityTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<PlayerHurtEntityTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(PlayerHurtEntityTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    damage: DamagePredicate::codec().optional_field_of("damage"),
                    entity: condition_holder_codec().optional_field_of("entity"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for PlayerHurtEntityTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(PlayerHurtEntityTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "entity",
        &this.entity,
    );
});

crate::register_triggers!(PlayerHurtEntityTriggerInstance {
    PLAYER_HURT_ENTITY = "player_hurt_entity",
});

impl PlayerHurtEntityTriggerInstance {
    /// `matches(player, victim, source, originalDamage, actualDamage, blocked)`.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        victim: &mut LootContext,
        source: &DamageSource,
        original_damage: f32,
        actual_damage: f32,
        blocked: bool,
    ) -> bool {
        if self.damage.as_ref().is_some_and(|d| {
            !d.matches(
                level,
                player,
                source,
                original_damage,
                actual_damage,
                blocked,
            )
        }) {
            return false;
        }
        holder_matches(&self.entity, victim)
    }
}

impl Trigger<PlayerHurtEntityTriggerInstance> {
    /// `trigger(player, victim, source, originalDamage, actualDamage, blocked)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        victim: &dyn Entity,
        source: &DamageSource,
        original_damage: f32,
        actual_damage: f32,
        blocked: bool,
    ) {
        let matches = level.expose(&*player, |level| {
            self.matches(
                &*level,
                &*player,
                victim,
                source,
                original_damage,
                actual_damage,
                blocked,
            )
        });
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        victim: &dyn Entity,
        source: &DamageSource,
        original_damage: f32,
        actual_damage: f32,
        blocked: bool,
    ) -> TriggerMatches {
        let params = entity_params(level, player.position(), victim);
        let mut context = new_context(&params);
        self.collect(level, player, |t| {
            t.matches(
                level,
                player,
                &mut context,
                source,
                original_damage,
                actual_damage,
                blocked,
            )
        })
    }
}
