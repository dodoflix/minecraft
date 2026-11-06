//! `ChanneledLightningTrigger`: `channeled_lightning`.

use super::prelude::*;
use crate::world::level::storage::loot::codec_ext::OptionalDefaultWith;

/// `ChanneledLightningTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct ChanneledLightningTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub victims: Vec<ConditionHolder>,
}

impl TriggerInstance for ChanneledLightningTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<ChanneledLightningTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(ChanneledLightningTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    victims: condition_holder_codec()
                        .list_of()
                        .optional_field_of_default_with("victims", Vec::new()),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for ChanneledLightningTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(ChanneledLightningTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_list(
        &validator.entity_context(),
        "victims",
        &this.victims,
    );
});

crate::register_triggers!(ChanneledLightningTriggerInstance {
    CHANNELED_LIGHTNING = "channeled_lightning",
});

impl ChanneledLightningTriggerInstance {
    /// `matches(victims)`: every victim condition is satisfied by some victim.
    pub fn matches(&self, victims: &mut [LootContext]) -> bool {
        self.victims.iter().all(|predicate| {
            let one = Some(predicate.clone());
            victims.iter_mut().any(|v| holder_matches(&one, v))
        })
    }
}

impl Trigger<ChanneledLightningTriggerInstance> {
    /// `trigger(player, victims)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        victims: &[&dyn Entity],
    ) {
        let matches = self.matches(&*level, &*player, victims);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        victims: &[&dyn Entity],
    ) -> TriggerMatches {
        let position = player.position();
        let params: Vec<_> = victims
            .iter()
            .map(|v| entity_params(level, position, *v))
            .collect();
        let mut contexts: Vec<LootContext> = params.iter().map(new_context).collect();
        self.collect(level, player, |t| t.matches(&mut contexts))
    }
}
