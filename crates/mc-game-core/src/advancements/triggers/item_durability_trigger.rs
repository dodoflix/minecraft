//! `ItemDurabilityTrigger`: `item_durability_changed`.

use super::prelude::*;

/// `ItemDurabilityTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct ItemDurabilityTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
    pub durability: Ints,
    pub delta: Ints,
}

impl TriggerInstance for ItemDurabilityTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<ItemDurabilityTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(ItemDurabilityTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                item: ItemPredicate::codec().optional_field_of("item"),
                durability: Ints::codec().optional_field_of_default("durability", Ints::any()),
                delta: Ints::codec().optional_field_of_default("delta", Ints::any()),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for ItemDurabilityTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(ItemDurabilityTriggerInstance);

crate::register_triggers!(ItemDurabilityTriggerInstance {
    ITEM_DURABILITY_CHANGED = "item_durability_changed",
});

impl ItemDurabilityTriggerInstance {
    /// `matches(itemStack, newDurability)`.
    pub fn matches(&self, item_stack: &ItemStack, new_durability: i32) -> bool {
        if self.item.as_ref().is_some_and(|p| !p.test(item_stack)) {
            return false;
        }
        self.durability.matches(item_stack.get_max_damage().wrapping_sub(new_durability)) && self.delta.matches(item_stack.get_damage_value().wrapping_sub(new_durability))
    }
}

impl Trigger<ItemDurabilityTriggerInstance> {
    /// `trigger(player, itemStack, newDurability)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, item_stack: &ItemStack, new_durability: i32) {
        self.trigger_with(level, player, |t| t.matches(item_stack, new_durability));
    }
}
