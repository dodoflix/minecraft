//! `MatchingBlocksPredicate` (`matching_blocks`).

use mc_core::pos::Vec3i;
use mc_data::block::BlockState;
use mc_game_core::world::level::levelgen::blockpredicates::BLOCK_PREDICATE_TYPES;
use mc_game_core::world::level::levelgen::blockpredicates::state_testing_predicate::{
    StateTestingPredicate, state_testing_codec,
};
use mc_game_core::world::level::levelgen::codec::{BlockSet, TypeEntry};
use std::sync::Arc;

pub struct MatchingBlocksPredicate {
    offset: Vec3i,
    blocks: BlockSet,
}

impl MatchingBlocksPredicate {
    pub fn new(offset: Vec3i, blocks: BlockSet) -> MatchingBlocksPredicate {
        MatchingBlocksPredicate { offset, blocks }
    }
}

impl StateTestingPredicate for MatchingBlocksPredicate {
    fn offset(&self) -> &Vec3i {
        &self.offset
    }

    fn test_state(&self, state: BlockState) -> bool {
        self.blocks.contains(state.block())
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static MATCHING_BLOCKS: TypeEntry<
    dyn mc_game_core::world::level::levelgen::blockpredicates::BlockPredicate,
> = TypeEntry {
    name: "matching_blocks",
    decode: |fields, _| {
        let offset = state_testing_codec(fields)?;
        let blocks = fields.required("blocks", BlockSet::from_json)?;
        Ok(Arc::new(MatchingBlocksPredicate { offset, blocks }))
    },
};
