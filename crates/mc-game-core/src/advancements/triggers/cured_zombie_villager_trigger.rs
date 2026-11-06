//! `CuredZombieVillagerTrigger`: `cured_zombie_villager`.

use super::prelude::*;

/// `CuredZombieVillagerTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct CuredZombieVillagerTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub zombie: Option<ConditionHolder>,
    pub villager: Option<ConditionHolder>,
}

impl TriggerInstance for CuredZombieVillagerTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<CuredZombieVillagerTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(CuredZombieVillagerTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    zombie: condition_holder_codec().optional_field_of("zombie"),
                    villager: condition_holder_codec().optional_field_of("villager"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for CuredZombieVillagerTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(CuredZombieVillagerTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "zombie",
        &this.zombie,
    );
    crate::world::level::storage::loot::validation::validate_holder_opt(
        &validator.entity_context(),
        "villager",
        &this.villager,
    );
});

crate::register_triggers!(CuredZombieVillagerTriggerInstance {
    CURED_ZOMBIE_VILLAGER = "cured_zombie_villager",
});

impl CuredZombieVillagerTriggerInstance {
    /// `matches(zombie, villager)`.
    pub fn matches(&self, zombie: &mut LootContext, villager: &mut LootContext) -> bool {
        holder_matches(&self.zombie, zombie) && holder_matches(&self.villager, villager)
    }
}

impl Trigger<CuredZombieVillagerTriggerInstance> {
    /// `trigger(player, zombie, villager)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        zombie: &dyn Entity,
        villager: &dyn Entity,
    ) {
        let matches = self.matches(&*level, &*player, zombie, villager);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        zombie: &dyn Entity,
        villager: &dyn Entity,
    ) -> TriggerMatches {
        let position = player.position();
        let zombie_params = entity_params(level, position, zombie);
        let villager_params = entity_params(level, position, villager);
        let mut zombie_context = new_context(&zombie_params);
        let mut villager_context = new_context(&villager_params);
        self.collect(level, player, |t| {
            t.matches(&mut zombie_context, &mut villager_context)
        })
    }
}
