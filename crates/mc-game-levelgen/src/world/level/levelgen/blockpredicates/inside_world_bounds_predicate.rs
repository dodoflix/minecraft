//! `InsideWorldBoundsPredicate` (`inside_world_bounds`).

use mc_core::pos::{BlockPos, Vec3i};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::blockpredicates::{
    BLOCK_PREDICATE_TYPES, BlockPredicate,
};
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_vec3i_offset};
use std::sync::Arc;

pub struct InsideWorldBoundsPredicate {
    pub offset: Vec3i,
}

impl BlockPredicate for InsideWorldBoundsPredicate {
    fn test(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        // `isInsideBuildHeight(pos)` == `!isOutsideBuildHeight(pos.getY())`
        !level.is_outside_build_height(pos.offset_vec(&self.offset).y())
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static INSIDE_WORLD_BOUNDS: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "inside_world_bounds",
    decode: |fields, _| {
        let offset = fields
            .optional("offset", |v| parse_vec3i_offset(v, 16))?
            .unwrap_or(Vec3i::ZERO);
        Ok(Arc::new(InsideWorldBoundsPredicate { offset }))
    },
};
