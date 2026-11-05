//! `ItemUsedOnLocationTrigger`: `placed_block`, `item_used_on_block`, `allay_drop_item_on_block`.

use super::prelude::*;
use crate::world::level::BlockGetter;
use crate::world::level::storage::loot::parameters::LootContextParamSets;
use crate::world::level::storage::loot::LootParams;
use crate::world::level::storage::loot::validation::validate_holder_opt;

/// `ItemUsedOnLocationTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct ItemUsedOnLocationTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub location: Option<ConditionHolder>,
}

impl TriggerInstance for ItemUsedOnLocationTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<ItemUsedOnLocationTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(ItemUsedOnLocationTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                location: condition_holder_codec().optional_field_of("location"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for ItemUsedOnLocationTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(ItemUsedOnLocationTriggerInstance, |this, validator| {
    validate_holder_opt(&validator.context(LootContextParamSets::ADVANCEMENT_LOCATION), "location", &this.location);
});

crate::register_triggers!(ItemUsedOnLocationTriggerInstance {
    PLACED_BLOCK = "placed_block",
    ITEM_USED_ON_BLOCK = "item_used_on_block",
    ALLAY_DROP_ITEM_ON_BLOCK = "allay_drop_item_on_block",
});

impl ItemUsedOnLocationTriggerInstance {
    /// `matches(locationContext)`.
    pub fn matches(&self, location_context: &mut LootContext) -> bool {
        holder_matches(&self.location, location_context)
    }
}

impl Trigger<ItemUsedOnLocationTriggerInstance> {
    /// `trigger(player, pos, tool)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, pos: BlockPos, tool: &ItemStack) {
        let matches = self.matches(&*level, &*player, pos, tool);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, pos: BlockPos, tool: &ItemStack) -> TriggerMatches {
        let state = level.get_block_state(pos);
        let params = LootParams::builder(level)
            .origin(Vec3::at_center_of(&pos))
            .this_entity(player)
            .block_state(state)
            .block_entity(level.get_block_entity(pos))
            .tool(tool)
            .create(&LootContextParamSets::ADVANCEMENT_LOCATION);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(&mut context))
    }
}
