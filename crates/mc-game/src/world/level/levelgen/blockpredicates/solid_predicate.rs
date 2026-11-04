//! `SolidPredicate` (`solid`, deprecated in vanilla).

use super::BLOCK_PREDICATE_TYPES;
use super::state_testing_predicate::{StateTestingPredicate, state_testing_codec};
use crate::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::Vec3i;
use mc_data::block::BlockState;
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
static SOLID: TypeEntry<dyn super::BlockPredicate> = TypeEntry {
    name: "solid",
    decode: |fields, _| Ok(Arc::new(SolidPredicate { offset: state_testing_codec(fields)? })),
};
