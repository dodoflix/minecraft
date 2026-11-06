//! `BrewedPotionTrigger`: `brewed_potion`.

use super::prelude::*;
use mc_protocol::components::{PotionContents, PotionsPredicate};

/// `BrewedPotionTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct BrewedPotionTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub potion: Option<PotionsPredicate>,
}

impl TriggerInstance for BrewedPotionTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<BrewedPotionTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(BrewedPotionTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    potion: PotionsPredicate::codec().optional_field_of("potion"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for BrewedPotionTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(BrewedPotionTriggerInstance);

crate::register_triggers!(BrewedPotionTriggerInstance {
    BREWED_POTION = "brewed_potion",
});

impl BrewedPotionTriggerInstance {
    /// `matches(potion)`.
    pub fn matches(&self, potion: &PotionContents) -> bool {
        self.potion.as_ref().is_none_or(|p| p.matches(potion))
    }
}

impl Trigger<BrewedPotionTriggerInstance> {
    /// `trigger(player, potion)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        potion: &PotionContents,
    ) {
        self.trigger_with(level, player, |t| t.matches(potion));
    }
}
