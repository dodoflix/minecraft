//! `AllOfPredicate` (`all_of`).

use super::combining_predicate::predicates_codec;
use mc_game_core::world::level::levelgen::blockpredicates::{BLOCK_PREDICATE_TYPES, BlockPredicate};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::BlockPos;
use std::sync::Arc;

pub struct AllOfPredicate {
    pub predicates: Vec<Arc<dyn BlockPredicate>>,
}

impl BlockPredicate for AllOfPredicate {
    fn test(&self, level: &dyn LevelAccessor, origin: BlockPos) -> bool {
        self.predicates.iter().all(|predicate| predicate.test(level, origin))
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static ALL_OF: TypeEntry<dyn BlockPredicate> =
    TypeEntry { name: "all_of", decode: |fields, ctx| Ok(Arc::new(AllOfPredicate { predicates: predicates_codec(fields, ctx)? })) };
