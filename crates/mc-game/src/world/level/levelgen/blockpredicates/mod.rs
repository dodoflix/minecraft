//! `net.minecraft.world.level.levelgen.blockpredicates`: `BlockPredicate` (`BiPredicate<LevelAccessor, BlockPos>`) and its
//! type table. Each predicate type lives in its own file and registers itself in [`BLOCK_PREDICATE_TYPES`].

use super::codec::{LoadResult, TypeEntry, decode_dispatch};
use super::registries::DecodeCtx;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use serde_json::Value;
use std::sync::Arc;

pub mod all_of_predicate;
pub mod any_of_predicate;
pub mod combining_predicate;
pub mod has_sturdy_face_predicate;
pub mod height_range_predicate;
pub mod inside_world_bounds_predicate;
pub mod matching_biomes_predicate;
pub mod matching_block_tag_predicate;
pub mod matching_blocks_predicate;
pub mod matching_fluids_predicate;
pub mod not_predicate;
pub mod replaceable_predicate;
pub mod solid_predicate;
pub mod state_testing_predicate;
pub mod true_block_predicate;
pub mod unobstructed_predicate;
pub mod volume_match_predicate;
pub mod would_survive_predicate;

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
