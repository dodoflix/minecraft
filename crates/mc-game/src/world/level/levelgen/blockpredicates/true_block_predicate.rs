//! `TrueBlockPredicate` (`true`): `MapCodec.unit`, any object decodes.

use super::{BLOCK_PREDICATE_TYPES, BlockPredicate};
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::codec::TypeEntry;
use mc_core::pos::BlockPos;
use std::sync::Arc;

pub struct TrueBlockPredicate;

impl BlockPredicate for TrueBlockPredicate {
    fn test(&self, _level: &dyn LevelAccessor, _origin: BlockPos) -> bool {
        true
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static TRUE: TypeEntry<dyn BlockPredicate> = TypeEntry { name: "true", decode: |_, _| Ok(Arc::new(TrueBlockPredicate)) };
