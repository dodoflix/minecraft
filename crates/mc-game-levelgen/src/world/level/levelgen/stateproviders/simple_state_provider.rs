//! `SimpleStateProvider` (`simple`).

use super::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use crate::world::level::LevelAccessor;
use crate::world::level::levelgen::codec::{TypeEntry, parse_block_state};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use std::sync::Arc;

pub struct SimpleStateProvider {
    pub state: BlockState,
}

impl BlockStateProvider for SimpleStateProvider {
    fn get_state(&self, _level: &dyn LevelAccessor, _random: &mut dyn RandomSource, _pos: BlockPos) -> BlockState {
        self.state
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static SIMPLE: TypeEntry<dyn BlockStateProvider> =
    TypeEntry { name: "simple", decode: |fields, _| Ok(Arc::new(SimpleStateProvider { state: fields.required("state", parse_block_state)? })) };
