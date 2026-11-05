//! `BeeNestDestroyedTrigger`: `bee_nest_destroyed`.

use super::prelude::*;
use mc_protocol::components::{RegistrySet, StatePropertiesPredicate};

/// `BeeNestDestroyedTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct BeeNestDestroyedTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub block: Option<RegistrySet<mc_data::Block>>,
    pub state: Option<StatePropertiesPredicate>,
    pub item: Option<ItemPredicate>,
    pub bees_inside: Ints,
}

impl TriggerInstance for BeeNestDestroyedTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<BeeNestDestroyedTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(BeeNestDestroyedTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                block: RegistrySet::<mc_data::Block>::codec().optional_field_of("blocks"),
                state: StatePropertiesPredicate::codec().optional_field_of("state"),
                item: ItemPredicate::codec().optional_field_of("item"),
                bees_inside: Ints::codec().optional_field_of_default("num_bees_inside", Ints::any()),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for BeeNestDestroyedTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(BeeNestDestroyedTriggerInstance);

crate::register_triggers!(BeeNestDestroyedTriggerInstance {
    BEE_NEST_DESTROYED = "bee_nest_destroyed",
});

impl BeeNestDestroyedTriggerInstance {
    /// `matches(state, itemStack, numBeesInside)`.
    pub fn matches(&self, state: BlockState, item_stack: &ItemStack, num_bees_inside: i32) -> bool {
        if self.block.as_ref().is_some_and(|b| !b.contains(state.block())) {
            return false;
        }
        if self.state.as_ref().is_some_and(|p| !p.matches(state)) {
            return false;
        }
        if self.item.as_ref().is_some_and(|p| !p.test(item_stack)) {
            return false;
        }
        self.bees_inside.matches(num_bees_inside)
    }
}

impl Trigger<BeeNestDestroyedTriggerInstance> {
    /// `trigger(player, state, itemStack, numBeesInside)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, state: BlockState, item_stack: &ItemStack, num_bees_inside: i32) {
        self.trigger_with(level, player, |t| t.matches(state, item_stack, num_bees_inside));
    }
}
