//! `net.minecraft.advancements.triggers.ConsumeItemTrigger`: `consume_item`.

use mc_dfu::Codec;

use super::{SimpleInstance, Trigger, TriggerInstance};
use crate::server::level::ServerLevel;
use crate::world::entity::player::ServerPlayer;
use crate::world::item::ItemStack;
use crate::world::level::storage::loot::predicates::{ConditionHolder, condition_holder_codec};
use mc_protocol::components::ItemPredicate;

/// `ConsumeItemTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct ConsumeItemTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub item: Option<ItemPredicate>,
}

impl ConsumeItemTriggerInstance {
    /// `matches(itemStack)`.
    pub fn matches(&self, item_stack: &ItemStack) -> bool {
        self.item.as_ref().is_none_or(|p| p.test(item_stack))
    }
}

impl TriggerInstance for ConsumeItemTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<ConsumeItemTriggerInstance>> =
            std::sync::LazyLock::new(|| {
                mc_dfu::record!(ConsumeItemTriggerInstance {
                    player: condition_holder_codec().optional_field_of("player"),
                    item: ItemPredicate::codec().optional_field_of("item"),
                })
                .codec()
            });
        C.clone()
    }
}

impl SimpleInstance for ConsumeItemTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(ConsumeItemTriggerInstance);

crate::register_triggers!(ConsumeItemTriggerInstance { CONSUME_ITEM = "consume_item" });

impl Trigger<ConsumeItemTriggerInstance> {
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
