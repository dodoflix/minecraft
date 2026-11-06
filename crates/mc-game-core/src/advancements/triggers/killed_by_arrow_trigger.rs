//! `KilledByArrowTrigger`: `killed_by_arrow`.

use super::prelude::*;
use crate::world::level::storage::loot::codec_ext::OptionalDefaultWith;

/// `KilledByArrowTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct KilledByArrowTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub victims: Vec<ConditionHolder>,
    pub unique_entity_types: Ints,
    pub fired_from_weapon: Option<ItemPredicate>,
}

impl TriggerInstance for KilledByArrowTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<KilledByArrowTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(KilledByArrowTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    victims: condition_holder_codec()
                        .list_of()
                        .optional_field_of_default_with("victims", Vec::new()),
                    unique_entity_types: Ints::codec()
                        .optional_field_of_default("unique_entity_types", Ints::any()),
                    fired_from_weapon: ItemPredicate::codec()
                        .optional_field_of("fired_from_weapon"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for KilledByArrowTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(KilledByArrowTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_list(
        &validator.entity_context(),
        "victims",
        &this.victims,
    );
});

crate::register_triggers!(KilledByArrowTriggerInstance {
    KILLED_BY_ARROW = "killed_by_arrow",
});

impl KilledByArrowTriggerInstance {
    /// `matches(victims, uniqueEntityTypes, firedFromWeapon)`.
    pub fn matches(
        &self,
        victims: &mut [LootContext],
        unique_entity_types: i32,
        fired_from_weapon: Option<&ItemStack>,
    ) -> bool {
        if let Some(weapon) = &self.fired_from_weapon
            && !fired_from_weapon.is_some_and(|w| weapon.test(w))
        {
            return false;
        }
        if !self.victims.is_empty() {
            let mut remaining: Vec<usize> = (0..victims.len()).collect();
            for predicate in &self.victims {
                let one = Some(predicate.clone());
                match remaining
                    .iter()
                    .position(|&i| holder_matches(&one, &mut victims[i]))
                {
                    Some(p) => {
                        remaining.remove(p);
                    }
                    None => return false,
                }
            }
        }
        self.unique_entity_types.matches(unique_entity_types)
    }
}

impl Trigger<KilledByArrowTriggerInstance> {
    /// `trigger(player, victims, firedByWeapon)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        victims: &[&dyn Entity],
        fired_by_weapon: Option<&ItemStack>,
    ) {
        let matches = self.matches(&*level, &*player, victims, fired_by_weapon);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        victims: &[&dyn Entity],
        fired_by_weapon: Option<&ItemStack>,
    ) -> TriggerMatches {
        let position = player.position();
        let mut types = std::collections::HashSet::new();
        for victim in victims {
            types.insert(victim.get_type());
        }
        let params: Vec<_> = victims
            .iter()
            .map(|v| entity_params(level, position, *v))
            .collect();
        let mut contexts: Vec<LootContext> = params.iter().map(new_context).collect();
        let unique = types.len() as i32;
        self.collect(level, player, |t| {
            t.matches(&mut contexts, unique, fired_by_weapon)
        })
    }
}
