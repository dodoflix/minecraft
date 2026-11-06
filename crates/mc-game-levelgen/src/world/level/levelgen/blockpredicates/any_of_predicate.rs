//! `AnyOfPredicate` (`any_of`).

use super::combining_predicate::predicates_codec;
use mc_core::pos::BlockPos;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::blockpredicates::{
    BLOCK_PREDICATE_TYPES, BlockPredicate,
};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use std::sync::Arc;

pub struct AnyOfPredicate {
    pub predicates: Vec<Arc<dyn BlockPredicate>>,
}

impl BlockPredicate for AnyOfPredicate {
    fn test(&self, level: &dyn LevelAccessor, origin: BlockPos) -> bool {
        self.predicates
            .iter()
            .any(|predicate| predicate.test(level, origin))
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static ANY_OF: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "any_of",
    decode: |fields, ctx| {
        Ok(Arc::new(AnyOfPredicate {
            predicates: predicates_codec(fields, ctx)?,
        }))
    },
};
