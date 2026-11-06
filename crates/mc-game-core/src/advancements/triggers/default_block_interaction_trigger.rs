//! `DefaultBlockInteractionTrigger`: `default_block_use`.

use super::prelude::*;
use crate::world::level::BlockGetter;
use crate::world::level::storage::loot::LootParams;
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::validation::validate_holder_opt;

/// `DefaultBlockInteractionTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct DefaultBlockInteractionTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub location: Option<ConditionHolder>,
}

impl TriggerInstance for DefaultBlockInteractionTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<DefaultBlockInteractionTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(DefaultBlockInteractionTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    location: condition_holder_codec().optional_field_of("location"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for DefaultBlockInteractionTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(DefaultBlockInteractionTriggerInstance, |this, validator| {
    validate_holder_opt(
        &validator.context(LootContextParamSets::BLOCK_USE),
        "location",
        &this.location,
    );
});

crate::register_triggers!(DefaultBlockInteractionTriggerInstance {
    DEFAULT_BLOCK_USE = "default_block_use",
});

impl DefaultBlockInteractionTriggerInstance {
    /// `matches(locationContext)`.
    pub fn matches(&self, location_context: &mut LootContext) -> bool {
        holder_matches(&self.location, location_context)
    }
}

impl Trigger<DefaultBlockInteractionTriggerInstance> {
    /// `trigger(player, pos)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, pos: BlockPos) {
        let matches = self.matches(&*level, &*player, pos);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(
        &self,
        level: &ServerLevel,
        player: &ServerPlayer,
        pos: BlockPos,
    ) -> TriggerMatches {
        let state = level.get_block_state(pos);
        let params = LootParams::builder(level)
            .origin(Vec3::at_center_of(&pos))
            .this_entity(player)
            .block_state(state)
            .create(&LootContextParamSets::BLOCK_USE);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(&mut context))
    }
}
