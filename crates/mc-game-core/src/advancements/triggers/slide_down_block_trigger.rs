//! `SlideDownBlockTrigger`: `slide_down_block`.

use super::prelude::*;
use mc_protocol::components::{RegistrySet, StatePropertiesPredicate};

/// `SlideDownBlockTrigger.TriggerInstance`.
#[derive(Clone)]
pub struct SlideDownBlockTriggerInstance {
    pub player: Option<ConditionHolder>,
    pub block: Option<RegistrySet<mc_data::Block>>,
    pub state: Option<StatePropertiesPredicate>,
}

impl TriggerInstance for SlideDownBlockTriggerInstance {
    fn codec() -> Codec<Self> {
        static C: std::sync::LazyLock<Codec<SlideDownBlockTriggerInstance>> = std::sync::LazyLock::new(|| {
            mc_dfu::record!(SlideDownBlockTriggerInstance {
                player: condition_holder_codec().optional_field_of("player"),
                block: RegistrySet::<mc_data::Block>::codec().optional_field_of("blocks"),
                state: StatePropertiesPredicate::codec().optional_field_of("state"),
            })
            .codec()
        });
        C.clone()
    }
}

impl SimpleInstance for SlideDownBlockTriggerInstance {
    fn player(&self) -> &Option<ConditionHolder> {
        &self.player
    }
}

crate::simple_instance!(SlideDownBlockTriggerInstance);

crate::register_triggers!(SlideDownBlockTriggerInstance {
    HONEY_BLOCK_SLIDE = "slide_down_block",
});

impl SlideDownBlockTriggerInstance {
    /// `matches(state)`.
    pub fn matches(&self, state: BlockState) -> bool {
        if self.block.as_ref().is_some_and(|b| !b.contains(state.block())) {
            return false;
        }
        self.state.as_ref().is_none_or(|p| p.matches(state))
    }
}

impl Trigger<SlideDownBlockTriggerInstance> {
    /// `trigger(player, state)`.
    pub fn trigger(&self, level: &mut ServerLevel, player: &mut ServerPlayer, state: BlockState) {
        self.trigger_with(level, player, |t| t.matches(state));
    }
}
