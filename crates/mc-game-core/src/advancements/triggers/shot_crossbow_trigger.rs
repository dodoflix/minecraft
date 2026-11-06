//! `ShotCrossbowTrigger`: `shot_crossbow`.

use super::prelude::*;

/// `ShotCrossbowTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct ShotCrossbowTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
}

impl TriggerInstance for ShotCrossbowTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<ShotCrossbowTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(ShotCrossbowTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    item: ItemPredicate::codec().optional_field_of("item"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for ShotCrossbowTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(ShotCrossbowTriggerInstance);

crate::register_triggers!(ShotCrossbowTriggerInstance {
    SHOT_CROSSBOW = "shot_crossbow",
});

impl ShotCrossbowTriggerInstance {
    /// `matches(itemStack)`.
    pub fn matches(&self, item_stack: &ItemStack) -> bool {
        self.item.as_ref().is_none_or(|p| p.test(item_stack))
    }
}

impl Trigger<ShotCrossbowTriggerInstance> {
    /// `trigger(player, itemStack)`.
    pub fn trigger(
        &self,
        level: &mut ServerLevel,
        player: &mut ServerPlayer,
        item_stack: &ItemStack,
    ) {
        self.trigger_with(level, player, |t| t.matches(item_stack));
    }
}
