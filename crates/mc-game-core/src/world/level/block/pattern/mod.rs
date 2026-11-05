//! `net.minecraft.world.level.block.state.pattern`: `BlockPattern`, `BlockPatternBuilder`, `BlockInWorld`.
//!
//! Vanilla's `BlockPatternMatch` keeps the `LevelReader` through its `LoadingCache<BlockPos, BlockInWorld>`; the Rust match
//! owns only the cache and every state read takes the level as an argument (`BlockInWorld::get_state(level)` loads
//! lazily exactly where vanilla does), so the level can be mutated between `find` and `get_block`.

mod block_in_world;
mod block_pattern;
mod block_pattern_builder;

pub use block_in_world::BlockInWorld;
pub use block_pattern::{BlockPattern, BlockPatternMatch};
pub use block_pattern_builder::BlockPatternBuilder;
