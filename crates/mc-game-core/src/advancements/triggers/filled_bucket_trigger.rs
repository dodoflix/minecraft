//! `FilledBucketTrigger`: `filled_bucket`.

use super::prelude::*;

/// `FilledBucketTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct FilledBucketTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
}

impl TriggerInstance for FilledBucketTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<FilledBucketTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(FilledBucketTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    item: ItemPredicate::codec().optional_field_of("item"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for FilledBucketTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(FilledBucketTriggerInstance);

crate::register_triggers!(FilledBucketTriggerInstance {
    FILLED_BUCKET = "filled_bucket",
});

impl FilledBucketTriggerInstance {
    /// `matches(item)`.
    pub fn matches(&self, item: &ItemStack) -> bool {
        self.item.as_ref().is_none_or(|p| p.test(item))
    }
}

impl Trigger<FilledBucketTriggerInstance> {
    /// `trigger(player, item)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, item: &ItemStack) {
        self.trigger_with(level, player, |t| t.matches(item));
    }
}
