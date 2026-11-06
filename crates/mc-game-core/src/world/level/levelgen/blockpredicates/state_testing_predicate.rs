//! `StateTestingPredicate`: a predicate on the block state at `origin + offset`.

use super::BlockPredicate;
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::codec::{Fields, LoadResult, parse_vec3i_offset};
use mc_core::pos::{BlockPos, Vec3i};
use mc_data::block::BlockState;

pub trait StateTestingPredicate: Send + Sync {
    fn offset(&self) -> &Vec3i;

    /// `test(BlockState)`.
    fn test_state(&self, state: BlockState) -> bool;
}

impl<T: StateTestingPredicate> BlockPredicate for T {
    fn test(&self, level: &dyn LevelAccessor, origin: BlockPos) -> bool {
        self.test_state(level.get_block_state(origin.offset_vec(self.offset())))
    }
}

/// `stateTestingCodec`: `Vec3i.offsetCodec(16).optionalFieldOf("offset", Vec3i.ZERO)`.
pub fn state_testing_codec(fields: &Fields<'_>) -> LoadResult<Vec3i> {
    Ok(fields
        .optional("offset", |v| parse_vec3i_offset(v, 16))?
        .unwrap_or(Vec3i::ZERO))
}
