//! `BlockPattern` and `BlockPatternMatch`.

use super::block_in_world::BlockInWorld;
use crate::world::level::LevelReader;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use std::collections::HashMap;
use std::sync::Arc;

/// `Predicate<BlockInWorld>` (the level is passed along because `BlockInWorld` does not hold it).
pub type Predicate = Arc<dyn Fn(&mut BlockInWorld, &dyn LevelReader) -> bool + Send + Sync>;

/// `LoadingCache<BlockPos, BlockInWorld>`.
#[derive(Default)]
pub struct BlockCache {
    load_chunks: bool,
    blocks: HashMap<BlockPos, BlockInWorld>,
}

impl BlockCache {
    /// `createLevelCache(level, loadChunks)`.
    pub fn new(load_chunks: bool) -> Self {
        BlockCache {
            load_chunks,
            blocks: HashMap::new(),
        }
    }

    fn get_unchecked(&mut self, pos: BlockPos) -> &mut BlockInWorld {
        let load_chunks = self.load_chunks;
        self.blocks
            .entry(pos)
            .or_insert_with(|| BlockInWorld::new(pos, load_chunks))
    }
}

pub struct BlockPattern {
    /// `pattern[depth][height][width]`.
    pattern: Vec<Vec<Vec<Predicate>>>,
    depth: i32,
    height: i32,
    width: i32,
}

impl BlockPattern {
    pub fn new(pattern: Vec<Vec<Vec<Predicate>>>) -> Self {
        let depth = pattern.len() as i32;
        let (height, width) = if depth > 0 {
            let height = pattern[0].len() as i32;
            (
                height,
                if height > 0 {
                    pattern[0][0].len() as i32
                } else {
                    0
                },
            )
        } else {
            (0, 0)
        };
        BlockPattern {
            pattern,
            depth,
            height,
            width,
        }
    }

    pub fn get_depth(&self) -> i32 {
        self.depth
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    pub fn get_width(&self) -> i32 {
        self.width
    }

    /// `matches(level, origin, forwards, up)`.
    pub fn matches(
        &self,
        level: &dyn LevelReader,
        origin: BlockPos,
        forwards: Direction,
        up: Direction,
    ) -> Option<BlockPatternMatch> {
        let mut cache = BlockCache::new(false);
        self.matches_cached(level, origin, forwards, up, &mut cache)
    }

    fn matches_cached(
        &self,
        level: &dyn LevelReader,
        origin: BlockPos,
        forwards: Direction,
        up: Direction,
        cache: &mut BlockCache,
    ) -> Option<BlockPatternMatch> {
        for x in 0..self.width {
            for y in 0..self.height {
                for z in 0..self.depth {
                    let block =
                        cache.get_unchecked(translate_and_rotate(origin, forwards, up, x, y, z));
                    if !(self.pattern[z as usize][y as usize][x as usize])(block, level) {
                        return None;
                    }
                }
            }
        }
        // the cache moves into the match; the caller's cache is reused for the next candidate only on failure
        Some(BlockPatternMatch {
            front_top_left: origin,
            forwards,
            up,
            cache: std::mem::take(cache),
            width: self.width,
            height: self.height,
            depth: self.depth,
        })
    }

    /// `find(level, origin)`.
    pub fn find(&self, level: &dyn LevelReader, origin: BlockPos) -> Option<BlockPatternMatch> {
        let mut cache = BlockCache::new(false);
        let dist = self.width.max(self.height).max(self.depth);
        for test_pos in
            BlockPos::between_closed(&origin, &origin.offset(dist - 1, dist - 1, dist - 1))
        {
            for forwards in Direction::VALUES {
                for up in Direction::VALUES {
                    if up != forwards
                        && up != forwards.get_opposite()
                        && let Some(found) =
                            self.matches_cached(level, test_pos, forwards, up, &mut cache)
                    {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
}

/// `translateAndRotate(origin, forwards, up, right, down, forwards)`.
pub fn translate_and_rotate(
    origin: BlockPos,
    forwards_direction: Direction,
    up_direction: Direction,
    right: i32,
    down: i32,
    forwards: i32,
) -> BlockPos {
    assert!(
        forwards_direction != up_direction && forwards_direction != up_direction.get_opposite(),
        "Invalid forwards & up combination"
    );
    let f = [
        forwards_direction.get_step_x(),
        forwards_direction.get_step_y(),
        forwards_direction.get_step_z(),
    ];
    let u = [
        up_direction.get_step_x(),
        up_direction.get_step_y(),
        up_direction.get_step_z(),
    ];
    // `Vec3i.cross`
    let r = [
        f[1] * u[2] - f[2] * u[1],
        f[2] * u[0] - f[0] * u[2],
        f[0] * u[1] - f[1] * u[0],
    ];
    origin.offset(
        u[0] * -down + r[0] * right + f[0] * forwards,
        u[1] * -down + r[1] * right + f[1] * forwards,
        u[2] * -down + r[2] * right + f[2] * forwards,
    )
}

pub struct BlockPatternMatch {
    front_top_left: BlockPos,
    forwards: Direction,
    up: Direction,
    cache: BlockCache,
    width: i32,
    height: i32,
    depth: i32,
}

impl BlockPatternMatch {
    pub fn get_front_top_left(&self) -> BlockPos {
        self.front_top_left
    }

    pub fn get_forwards(&self) -> Direction {
        self.forwards
    }

    pub fn get_up(&self) -> Direction {
        self.up
    }

    pub fn get_width(&self) -> i32 {
        self.width
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    pub fn get_depth(&self) -> i32 {
        self.depth
    }

    /// `getBlock(right, down, forwards)`.
    pub fn get_block(&mut self, right: i32, down: i32, forwards: i32) -> &mut BlockInWorld {
        self.cache.get_unchecked(translate_and_rotate(
            self.front_top_left,
            self.forwards,
            self.up,
            right,
            down,
            forwards,
        ))
    }
}
