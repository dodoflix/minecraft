//! `BlockInWorld`.

use crate::world::block_entity::BlockEntity;
use crate::world::level::LevelReader;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};

/// `BlockInWorld`: a position whose state is read lazily (and cached) from the level it is asked with.
#[derive(Clone, Debug)]
pub struct BlockInWorld {
    pos: BlockPos,
    load_chunks: bool,
    state: Option<BlockState>,
}

impl BlockInWorld {
    pub fn new(pos: BlockPos, load_chunks: bool) -> Self {
        BlockInWorld {
            pos,
            load_chunks,
            state: None,
        }
    }

    /// `getState()`: `None` (vanilla `null`) when the chunk is not loaded and loading is not requested.
    pub fn get_state(&mut self, level: &dyn LevelReader) -> Option<BlockState> {
        if self.state.is_none()
            && (self.load_chunks || level.has_chunk(self.pos.x() >> 4, self.pos.z() >> 4))
        {
            self.state = Some(level.get_block_state(self.pos));
        }
        self.state
    }

    /// `getEntity()` (not cached: callers read it once).
    pub fn get_entity<'a>(&self, level: &'a dyn LevelReader) -> Option<&'a dyn BlockEntity> {
        level.get_block_entity(self.pos)
    }

    pub fn get_pos(&self) -> BlockPos {
        self.pos
    }

    /// `BlockPredicate.forBlock(block)` applied to this block: `state != null && state.is(block)`.
    pub fn is_block(&mut self, level: &dyn LevelReader, block: Block) -> bool {
        self.get_state(level).is_some_and(|s| s.block() == block)
    }
}
