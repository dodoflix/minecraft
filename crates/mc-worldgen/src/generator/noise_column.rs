//! `net.minecraft.world.level.NoiseColumn` and `net.minecraft.world.level.chunk.BlockColumn`.

use mc_data::block::{Block, BlockState};

/// `BlockColumn`.
pub trait BlockColumn {
    fn get_block(&self, block_y: i32) -> BlockState;

    /// Panics outside the column like vanilla throws `IllegalArgumentException("Outside of column height: y")`.
    fn set_block(&mut self, block_y: i32, state: BlockState);
}

/// `NoiseColumn`: the block states of one column starting at `min_y`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoiseColumn {
    min_y: i32,
    column: Vec<BlockState>,
}

impl NoiseColumn {
    pub fn new(min_y: i32, column: Vec<BlockState>) -> NoiseColumn {
        NoiseColumn { min_y, column }
    }

    pub fn min_y(&self) -> i32 {
        self.min_y
    }

    /// The raw column (index = `y - min_y`).
    pub fn states(&self) -> &[BlockState] {
        &self.column
    }
}

impl BlockColumn for NoiseColumn {
    fn get_block(&self, block_y: i32) -> BlockState {
        let y_index = block_y.wrapping_sub(self.min_y);
        if y_index >= 0 && (y_index as usize) < self.column.len() {
            self.column[y_index as usize]
        } else {
            Block::AIR.default_state()
        }
    }

    fn set_block(&mut self, block_y: i32, state: BlockState) {
        let y_index = block_y.wrapping_sub(self.min_y);
        if y_index >= 0 && (y_index as usize) < self.column.len() {
            self.column[y_index as usize] = state;
        } else {
            panic!("Outside of column height: {block_y}");
        }
    }
}
