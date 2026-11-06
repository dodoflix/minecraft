//! `EnchantedItemTrigger`: `enchanted_item`.

use super::prelude::*;

/// `EnchantedItemTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct EnchantedItemTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
    pub levels: Ints,
}

impl TriggerInstance for EnchantedItemTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<EnchantedItemTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(EnchantedItemTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    item: ItemPredicate::codec().optional_field_of("item"),
                    levels: Ints::codec().optional_field_of_default("levels", Ints::any()),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for EnchantedItemTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(EnchantedItemTriggerInstance);

crate::register_triggers!(EnchantedItemTriggerInstance {
    ENCHANTED_ITEM = "enchanted_item",
});

impl EnchantedItemTriggerInstance {
    /// `matches(itemStack, levels)`.
    pub fn matches(&self, item_stack: &ItemStack, levels: i32) -> bool {
        if self.item.as_ref().is_some_and(|p| !p.test(item_stack)) {
            return false;
        }
        self.levels.matches(levels)
    }
}

impl Trigger<EnchantedItemTriggerInstance> {
    /// `trigger(player, itemStack, levels)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        item_stack: &ItemStack,
        levels: i32,
    ) {
        self.trigger_with(level, player, |t| t.matches(item_stack, levels));
    }
}
