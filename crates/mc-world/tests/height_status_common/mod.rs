//! Shared test helpers.
#![allow(dead_code, reason = "each test binary uses a subset of the shared helpers")]

use mc_data::block::BlockState;
use mc_data::Block;
use mc_world::heightmap::HeightmapChunk;

/// Dense block array; section `hasOnlyAir` = no state with the `isAir` flag (what `LevelChunkSection` counts).
pub struct TestChunk {
    pub min_y: i32,
    pub height: i32,
    pub states: Vec<BlockState>,
}

impl TestChunk {
    pub fn new(min_y: i32, height: i32) -> Self {
        TestChunk { min_y, height, states: vec![Block::AIR.default_state(); 256 * height as usize] }
    }
    pub fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((y - self.min_y) * 256 + z * 16 + x) as usize
    }
    pub fn section_has_only_air(&self, section_index: i32) -> bool {
        let base = section_index as usize * 16 * 256;
        self.states[base..base + 16 * 256].iter().all(|s| s.is_air())
    }
}

impl HeightmapChunk for TestChunk {
    fn min_y(&self) -> i32 {
        self.min_y
    }
    fn height(&self) -> i32 {
        self.height
    }
    fn highest_section_position(&self) -> i32 {
        (0..self.height / 16)
            .rev()
            .find(|&i| !self.section_has_only_air(i))
            .map_or(self.min_y, |i| self.min_y + i * 16)
    }
    fn get_block_state(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.states[self.idx(x, y, z)]
    }
}

