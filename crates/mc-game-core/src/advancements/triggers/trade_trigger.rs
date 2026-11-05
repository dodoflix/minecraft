//! `TradeTrigger`: `villager_trade`.

use super::prelude::*;

/// `TradeTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct TradeTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub villager: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
}

impl TriggerInstance for TradeTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<TradeTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(TradeTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                villager: condition_holder_codec().optional_field_of("villager"),
                item: ItemPredicate::codec().optional_field_of("item"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for TradeTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(TradeTriggerInstance, |this, validator| {
    crate::world::level::storage::loot::validation::validate_holder_opt(&validator.entity_context(), "villager", &this.villager);
});

crate::register_triggers!(TradeTriggerInstance {
    TRADE = "villager_trade",
});

impl TradeTriggerInstance {
    /// `matches(villager, itemStack)`.
    pub fn matches(&self, villager: &mut LootContext, item_stack: &ItemStack) -> bool {
        holder_matches(&self.villager, villager) && self.item.as_ref().is_none_or(|p| p.test(item_stack))
    }
}

impl Trigger<TradeTriggerInstance> {
    /// `trigger(player, villager, itemStack)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, villager: &dyn Entity, item_stack: &ItemStack) {
        let matches = self.matches(&*level, &*player, villager, item_stack);
        matches.award(level, player);
    }

    /// The listeners that match; nothing is mutated.
    pub fn matches(&self, level: &ServerLevel, player: &ServerPlayer, villager: &dyn Entity, item_stack: &ItemStack) -> TriggerMatches {
        let params = entity_params(level, player.position(), villager);
        let mut context = new_context(&params);
        self.collect(level, player, |t| t.matches(&mut context, item_stack))
    }
}
