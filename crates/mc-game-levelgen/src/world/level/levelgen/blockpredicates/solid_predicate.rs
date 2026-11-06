//! `SolidPredicate` (`solid`, deprecated in vanilla).

use mc_core::pos::Vec3i;
use mc_data::block::BlockState;
use mc_game_core::world::level::levelgen::blockpredicates::BLOCK_PREDICATE_TYPES;
use mc_game_core::world::level::levelgen::blockpredicates::state_testing_predicate::{
    StateTestingPredicate, state_testing_codec,
};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use std::sync::Arc;

pub struct SolidPredicate {
    pub offset: Vec3i,
}

impl StateTestingPredicate for SolidPredicate {
    fn offset(&self) -> &Vec3i {
        &self.offset
    }

    fn test_state(&self, state: BlockState) -> bool {
        state.is_solid()
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static SOLID: TypeEntry<dyn mc_game_core::world::level::levelgen::blockpredicates::BlockPredicate> =
    TypeEntry {
        name: "solid",
        decode: |fields, _| {
            Ok(Arc::new(SolidPredicate {
                offset: state_testing_codec(fields)?,
            }))
        },
    };
