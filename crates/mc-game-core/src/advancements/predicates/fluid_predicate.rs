//! `net.minecraft.advancements.predicates.FluidPredicate` (+ `StatePropertiesPredicate.matches(FluidState)`).

use crate::server::level::ServerLevel;
use crate::world::level::{BlockGetter, LevelReader};
use mc_core::pos::BlockPos;
use mc_data::fluid::FluidState;
use mc_data::props::{PropertyDef, PropertyKind};
use mc_dfu::Codec;
use mc_protocol::components::{RegistrySet, StatePropertiesPredicate};
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct FluidPredicate {
    pub fluids: Option<RegistrySet<mc_data::Fluid>>,
    pub properties: Option<StatePropertiesPredicate>,
}

/// `FlowingFluid.FALLING`.
static FALLING: PropertyDef = PropertyDef {
    name: "falling",
    kind: PropertyKind::Boolean,
    values: &["true", "false"],
    min: 0,
    enum_class: None,
};
/// `FlowingFluid.LEVEL` (`BlockStateProperties.LEVEL_FLOWING`).
static LEVEL: PropertyDef = PropertyDef {
    name: "level",
    kind: PropertyKind::Integer,
    values: &["1", "2", "3", "4", "5", "6", "7", "8"],
    min: 1,
    enum_class: None,
};

/// `StatePropertiesPredicate.matches(FluidState)`: a name that is not a property of the fluid never matches.
pub trait StatePropertiesPredicateFluidExt {
    fn matches_fluid(&self, state: FluidState) -> bool;
}

impl StatePropertiesPredicateFluidExt for StatePropertiesPredicate {
    fn matches_fluid(&self, state: FluidState) -> bool {
        self.properties.iter().all(|matcher| {
            let Some((_, value)) = state
                .data()
                .properties
                .iter()
                .find(|(name, _)| *name == matcher.name)
            else {
                return false;
            };
            let def: &PropertyDef = if matcher.name == "falling" {
                &FALLING
            } else {
                &LEVEL
            };
            def.value_index(value)
                .is_some_and(|index| matcher.value_matcher.matches(def, index))
        })
    }
}

impl FluidPredicate {
    pub fn codec() -> Codec<FluidPredicate> {
        static C: LazyLock<Codec<FluidPredicate>> = LazyLock::new(|| {
            mc_dfu::record!(FluidPredicate {
                fluids: RegistrySet::<mc_data::Fluid>::codec().optional_field_of("fluids"),
                properties: StatePropertiesPredicate::codec().optional_field_of("state"),
            })
            .codec()
        });
        C.clone()
    }

    pub fn matches(&self, level: &ServerLevel, pos: BlockPos) -> bool {
        if !level.is_loaded(pos) {
            return false;
        }
        let state = level.get_fluid_state(pos);
        self.fluids
            .as_ref()
            .is_none_or(|f| f.contains(state.fluid()))
            && self
                .properties
                .as_ref()
                .is_none_or(|p| p.matches_fluid(state))
    }
}
