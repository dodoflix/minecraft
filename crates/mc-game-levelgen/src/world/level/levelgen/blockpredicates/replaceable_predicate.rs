//! `ReplaceablePredicate` (`replaceable`).

use mc_game_core::world::level::levelgen::blockpredicates::BLOCK_PREDICATE_TYPES;
use mc_game_core::world::level::levelgen::blockpredicates::state_testing_predicate::{StateTestingPredicate, state_testing_codec};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::Vec3i;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct ReplaceablePredicate {
    pub offset: Vec3i,
}

impl StateTestingPredicate for ReplaceablePredicate {
    fn offset(&self) -> &Vec3i {
        &self.offset
    }

    fn test_state(&self, state: BlockState) -> bool {
        state.can_be_replaced()
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static REPLACEABLE: TypeEntry<dyn mc_game_core::world::level::levelgen::blockpredicates::BlockPredicate> = TypeEntry {
    name: "replaceable",
    decode: |fields, _| Ok(Arc::new(ReplaceablePredicate { offset: state_testing_codec(fields)? })),
};
