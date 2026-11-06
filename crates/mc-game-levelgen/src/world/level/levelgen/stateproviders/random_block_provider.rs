//! `RandomBlockProvider` (`random_block`).

use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_game_core::world::level::LevelAccessor;
use mc_game_core::world::level::levelgen::codec::{BlockSet, TypeEntry};
use mc_game_core::world::level::levelgen::stateproviders::{
    BLOCK_STATE_PROVIDER_TYPES, BlockStateProvider,
};
use std::sync::Arc;

pub struct RandomBlockProvider {
    /// `HolderSet<Block>.contents()` in vanilla order.
    blocks: Vec<Block>,
}

impl RandomBlockProvider {
    /// `getState(random)`: `blocks.getRandomElement(random)` (`Util.getRandomSafe`: no draw for an empty set).
    fn get_state_random(&self, random: &mut dyn RandomSource) -> Option<BlockState> {
        if self.blocks.is_empty() {
            return None;
        }
        Some(
            self.blocks[random.next_int_bounded(self.blocks.len() as i32) as usize].default_state(),
        )
    }
}

impl BlockStateProvider for RandomBlockProvider {
    fn get_state(
        &self,
        level: &dyn LevelAccessor,
        random: &mut dyn RandomSource,
        pos: BlockPos,
    ) -> BlockState {
        self.get_state_random(random)
            .unwrap_or_else(|| level.get_block_state(pos))
    }

    fn get_optional_state(
        &self,
        _level: &dyn LevelAccessor,
        random: &mut dyn RandomSource,
        _pos: BlockPos,
    ) -> Option<BlockState> {
        self.get_state_random(random)
    }
}

#[linkme::distributed_slice(BLOCK_STATE_PROVIDER_TYPES)]
static RANDOM_BLOCK: TypeEntry<dyn BlockStateProvider> = TypeEntry {
    name: "random_block",
    decode: |fields, _| {
        let blocks = fields.required("blocks", BlockSet::from_json)?;
        Ok(Arc::new(RandomBlockProvider {
            blocks: blocks.ordered_blocks(),
        }))
    },
};
