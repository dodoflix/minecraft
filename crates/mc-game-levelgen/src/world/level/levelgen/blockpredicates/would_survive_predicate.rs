//! `WouldSurvivePredicate` (`would_survive`).

use mc_game_core::world::level::levelgen::blockpredicates::{BLOCK_PREDICATE_TYPES, BlockPredicate};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::block::BlockStateExt;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_block_state, parse_vec3i_offset};
use mc_core::pos::{BlockPos, Vec3i};
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct WouldSurvivePredicate {
    pub offset: Vec3i,
    pub state: BlockState,
}

impl BlockPredicate for WouldSurvivePredicate {
    fn test(&self, level: &dyn LevelAccessor, origin: BlockPos) -> bool {
        self.state.can_survive(level, origin.offset_vec(&self.offset))
    }
}

#[linkme::distributed_slice(BLOCK_PREDICATE_TYPES)]
static WOULD_SURVIVE: TypeEntry<dyn BlockPredicate> = TypeEntry {
    name: "would_survive",
    decode: |fields, _| {
        let offset = fields.optional("offset", |v| parse_vec3i_offset(v, 16))?.unwrap_or(Vec3i::ZERO);
        let state = fields.required("state", parse_block_state)?;
        Ok(Arc::new(WouldSurvivePredicate { offset, state }))
    },
};
