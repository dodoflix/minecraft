//! `net.minecraft.advancements.triggers.ImpossibleTrigger`: never fires (advancements granted by commands/functions).

use std::any::Any;

use mc_dfu::{Codec, MapCodec};

use super::{CriterionTriggerInstance, TriggerInstance};
use crate::world::level::storage::loot::validation::ValidationContextSource;

/// `ImpossibleTrigger.TriggerInstance`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImpossibleTriggerInstance;

impl CriterionTriggerInstance for ImpossibleTriggerInstance {
    fn validate(&self, _validator: &ValidationContextSource) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl TriggerInstance for ImpossibleTriggerInstance {
    fn codec() -> Codec<Self> {
        MapCodec::unit(ImpossibleTriggerInstance).codec()
    }
}

crate::register_triggers!(ImpossibleTriggerInstance { IMPOSSIBLE = "impossible" });
