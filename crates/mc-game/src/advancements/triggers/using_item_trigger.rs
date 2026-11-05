//! `UsingItemTrigger`: `using_item`.

use super::prelude::*;

/// `UsingItemTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct UsingItemTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
}

impl TriggerInstance for UsingItemTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<UsingItemTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(UsingItemTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                item: ItemPredicate::codec().optional_field_of("item"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for UsingItemTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(UsingItemTriggerInstance);

crate::register_triggers!(UsingItemTriggerInstance {
    USING_ITEM = "using_item",
});

impl UsingItemTriggerInstance {
    /// `matches(item)`.
    pub fn matches(&self, item: &ItemStack) -> bool {
        self.item.as_ref().is_none_or(|p| p.test(item))
    }
}

impl Trigger<UsingItemTriggerInstance> {
    /// `trigger(player, item)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, item: &ItemStack) {
        self.trigger_with(level, player, |t| t.matches(item));
    }
}
