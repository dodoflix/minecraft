//! `net.minecraft.world.level.levelgen.blockpredicates`: `BlockPredicate` (`BiPredicate<LevelAccessor, BlockPos>`) and its
//! type table. Each predicate type lives in its own file and registers itself in [`BLOCK_PREDICATE_TYPES`].

use super::codec::{LoadResult, TypeEntry, decode_dispatch};
use super::registries::DecodeCtx;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use serde_json::Value;
use std::sync::Arc;

pub mod state_testing_predicate;

pub trait BlockPredicate: Send + Sync {
    /// `test(level, pos)`.
    fn test(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool;
}

#[linkme::distributed_slice]
pub static BLOCK_PREDICATE_TYPES: [TypeEntry<dyn BlockPredicate>];

/// `BlockPredicate.CODEC`.
pub fn decode(value: &Value, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn BlockPredicate>> {
    decode_dispatch(value, ctx, &BLOCK_PREDICATE_TYPES)
}
