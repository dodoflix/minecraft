//! `UnobstructedPredicate` (`unobstructed`). Unlike the other offsets, this codec's `offset` is an unbounded `Vec3i.CODEC`.

use super::{BLOCK_PREDICATE_TYPES, BlockPredicate};
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::codec::{TypeEntry, parse_vec3i};
use mc_core::pos::{BlockPos, Vec3i};
use std::sync::Arc;

pub struct UnobstructedPredicate {
    pub offset: Vec3i,
}

impl BlockPredicate for UnobstructedPredicate {
    /// `isUnobstructed(null, Shapes.block().move(pos))`: the vanilla predicate does NOT apply `offset`.
    fn test(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        level.is_unobstructed_block(pos)
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static UNOBSTRUCTED: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "unobstructed",
    decode: |fields, _| {
        let offset = fields.optional("offset", parse_vec3i)?.unwrap_or(Vec3i::ZERO);
        Ok(Arc::new(UnobstructedPredicate { offset }))
    },
};
