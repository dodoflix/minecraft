//! `TrueBlockPredicate` (`true`): `MapCodec.unit`, any object decodes.

use mc_core::pos::BlockPos;
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::blockpredicates::{
    BLOCK_PREDICATE_TYPES, BlockPredicate,
};
use mc_game_core::world::level::levelgen::codec::TypeEntry;
use std::sync::Arc;

pub struct TrueBlockPredicate;

impl BlockPredicate for TrueBlockPredicate {
    fn test(&self, _level: &dyn LevelAccessor, _origin: BlockPos) -> bool {
        true
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static TRUE: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "true",
    decode: |_, _| Ok(Arc::new(TrueBlockPredicate)),
};
