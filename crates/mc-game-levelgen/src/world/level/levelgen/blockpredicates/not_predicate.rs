//! `NotPredicate` (`not`).

use mc_core::pos::BlockPos;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::blockpredicates::{
    BLOCK_PREDICATE_TYPES, BlockPredicate, decode,
};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use std::sync::Arc;

pub struct NotPredicate {
    pub predicate: Arc<dyn BlockPredicate>,
}

impl BlockPredicate for NotPredicate {
    fn test(&self, level: &dyn LevelAccessor, origin: BlockPos) -> bool {
        !self.predicate.test(level, origin)
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static NOT: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "not",
    decode: |fields, ctx| {
        Ok(Arc::new(NotPredicate {
            predicate: fields.required("predicate", |v| decode(v, ctx))?,
        }))
    },
};
