//! `MatchingFluidsPredicate` (`matching_fluids`).

use mc_game_core::world::level::levelgen::blockpredicates::BLOCK_PREDICATE_TYPES;
use mc_game_core::world::level::levelgen::blockpredicates::state_testing_predicate::{StateTestingPredicate, state_testing_codec};
use mc_game_core::world::level::levelgen::codec::{FluidSet, TypeEntry};
use mc_core::pos::Vec3i;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct MatchingFluidsPredicate {
    offset: Vec3i,
    fluids: FluidSet,
}

impl MatchingFluidsPredicate {
    pub fn new(offset: Vec3i, fluids: FluidSet) -> MatchingFluidsPredicate {
        MatchingFluidsPredicate { offset, fluids }
    }
}

impl StateTestingPredicate for MatchingFluidsPredicate {
    fn offset(&self) -> &Vec3i {
        &self.offset
    }

    fn test_state(&self, state: BlockState) -> bool {
        self.fluids.contains(state.get_fluid_state().fluid())
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static MATCHING_FLUIDS: TypeEntry<dyn mc_game_core::world::level::levelgen::blockpredicates::BlockPredicate> = TypeEntry {
    name: "matching_fluids",
    decode: |fields, _| {
        let offset = state_testing_codec(fields)?;
        let fluids = fields.required("fluids", FluidSet::from_json)?;
        Ok(Arc::new(MatchingFluidsPredicate { offset, fluids }))
    },
};
