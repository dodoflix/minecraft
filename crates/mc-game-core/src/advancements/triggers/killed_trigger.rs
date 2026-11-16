//! `KilledTrigger`: `player_killed_entity`, `entity_killed_player`, `kill_mob_near_sculk_catalyst`.

use super::prelude::*;
use crate::advancements::predicates::DamageSourcePredicate;

/// `KilledTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct KilledTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub entity: Option<ConditionHolder>,
    pub killing_blow: Option<DamageSourcePredicate>,
}

impl TriggerInstance for KilledTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<KilledTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(KilledTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    entity: condition_holder_codec().optional_field_of("entity"),
                    killing_blow: DamageSourcePredicate::codec().optional_field_of("killing_blow"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for KilledTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(KilledTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "entity",
        &this.entity,
    );
});

crate::register_triggers!(KilledTriggerInstance {
    PLAYER_KILLED_ENTITY = "player_killed_entity",
    ENTITY_KILLED_PLAYER = "entity_killed_player",
    KILL_MOB_NEAR_SCULK_CATALYST = "kill_mob_near_sculk_catalyst",
});

impl KilledTriggerInstance {
    /// `matches(player, entity, killingBlow)`. `player` and `victim` (the entity of the context: the killer for `entity_killed_player`)
    /// are both running code's entities, out of the level's storage: when the killing blow names one of them it is taken from here.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        victim: &dyn Entity,
        entity: &mut LootContext,
        killing_blow: &DamageSource,
    ) -> bool {
        if self.killing_blow.as_ref().is_some_and(|p| {
            let find = |id| {
                if id == player.id() {
                    Some(player as &dyn Entity)
                } else if id == victim.id() {
                    Some(victim)
                } else {
                    level.get_entity_or_recently_removed(id)
                }
            };
            let causing = killing_blow.get_entity().and_then(find);
            let direct = killing_blow.get_direct_entity().and_then(find);
            !p.matches_parts(
                level,
                Some(player.position()),
                &killing_blow.damage_type,
                direct,
                causing,
                killing_blow.is_direct(),
            )
        }) {
            return false;
        }
        holder_matches(&self.entity, entity)
    }
}

impl Trigger<KilledTriggerInstance> {
    /// `trigger(player, entity, killingBlow)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        entity: &dyn Entity,
        killing_blow: &DamageSource,
    ) {
        let matches = self.matches(&*level, &*player, entity, killing_blow);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        entity: &dyn Entity,
        killing_blow: &DamageSource,
    ) -> TriggerMatches {
        let params = entity_params(level, player.position(), entity);
        let mut context = new_context(&params);
        self.collect(level, player, |t| {
            t.matches(level, player, entity, &mut context, killing_blow)
        })
    }
}
