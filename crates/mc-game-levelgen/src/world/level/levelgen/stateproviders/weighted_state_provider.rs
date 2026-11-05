//! `WeightedStateProvider` (`weighted`).

use mc_game_core::world::level::levelgen::stateproviders::{BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::{TypeEntry, parse_block_state};
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::BlockState;
use mc_worldgen::provider::WeightedList;
use std::sync::Arc;

pub struct WeightedStateProvider {
    pub weighted_list: WeightedList<BlockState>,
}

impl BlockStateProvider for WeightedStateProvider {
    fn get_state(&self, _level: &dyn LevelAccessor, random: &mut dyn RandomSource, _pos: BlockPos) -> BlockState {
        *self.weighted_list.get_random_or_throw(random)
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static WEIGHTED: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "weighted",
    decode: |fields, _| {
        let weighted_list = fields.required("entries", |v| WeightedList::non_empty_from_json(v, parse_block_state))?;
        Ok(Arc::new(WeightedStateProvider { weighted_list }))
    },
};
