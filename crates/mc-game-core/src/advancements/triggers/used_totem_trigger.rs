//! `UsedTotemTrigger`: `used_totem`.

use super::prelude::*;

/// `UsedTotemTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct UsedTotemTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
}

impl TriggerInstance for UsedTotemTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<UsedTotemTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(UsedTotemTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                item: ItemPredicate::codec().optional_field_of("item"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for UsedTotemTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(UsedTotemTriggerInstance);

crate::register_triggers!(UsedTotemTriggerInstance {
    USED_TOTEM = "used_totem",
});

impl UsedTotemTriggerInstance {
    /// `matches(itemStack)`.
    pub fn matches(&self, item_stack: &ItemStack) -> bool {
        self.item.as_ref().is_none_or(|p| p.test(item_stack))
    }
}

impl Trigger<UsedTotemTriggerInstance> {
    /// `trigger(player, itemStack)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, item_stack: &ItemStack) {
        self.trigger_with(level, player, |t| t.matches(item_stack));
    }
}
