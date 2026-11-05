//! `EnterBlockTrigger`: `enter_block`.

use super::prelude::*;
use mc_protocol::components::{PotionContents, PotionsPredicate, RegistrySet, StatePropertiesPredicate};
use crate::world::container::Container;

/// `EnterBlockTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct EnterBlockTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub block: Option<RegistrySet<mc_data::Block>>,
    pub state: Option<StatePropertiesPredicate>,
}

impl TriggerInstance for EnterBlockTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<EnterBlockTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(EnterBlockTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                block: RegistrySet::<mc_data::Block>::codec().optional_field_of("blocks"),
                state: StatePropertiesPredicate::codec().optional_field_of("state"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for EnterBlockTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(EnterBlockTriggerInstance);

crate::register_triggers!(EnterBlockTriggerInstance {
    ENTER_BLOCK = "enter_block",
});

impl EnterBlockTriggerInstance {
    /// `matches(state)`.
    pub fn matches(&self, state: BlockState) -> bool {
        if self.block.as_ref().is_some_and(|b| !b.contains(state.block())) {
            return false;
        }
        self.state.as_ref().is_none_or(|p| p.matches(state))
    }
}

impl Trigger<EnterBlockTriggerInstance> {
    /// `trigger(player, state)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, state: BlockState) {
        self.trigger_with(level, player, |t| t.matches(state));
    }
}
