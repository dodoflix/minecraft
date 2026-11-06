//! `LightChunk` / `LightChunkGetter`: what the light engine needs from chunks.
//!
//! Vanilla's engines hold the `LightChunkGetter` (the chunk cache). Here the getter is passed into every call that
//! reads chunks (`run_light_updates`, `set_light_enabled`, `propagate_light_sources`), so the engine owns no chunk
//! references. The caller chooses the locking: the getter only has to keep the returned chunks alive for the call.

use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::BlockState;
use mc_world::chunk::{ChunkAccess, LevelChunk, LevelHeight, ProtoChunk};
use mc_world::sky_light_sources::ChunkSkyLightSources;
use std::collections::HashMap;

/// `LightChunk` (a `BlockGetter` that can enumerate its block light sources).
pub trait LightChunk {
    fn get_block_state(&self, pos: BlockPos) -> BlockState;
    /// `findBlockLightSources(consumer)`.
    fn find_block_light_sources(&self, consumer: &mut dyn FnMut(BlockPos, BlockState));
    /// `getSkyLightSources()`.
    fn get_sky_light_sources(&self) -> &ChunkSkyLightSources;
}

fn find_sources(access: &ChunkAccess, consumer: &mut dyn FnMut(BlockPos, BlockState)) {
    access.find_blocks(|state| state.get_light_emission() != 0, consumer);
}

impl LightChunk for ProtoChunk {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        ProtoChunk::get_block_state(self, pos)
    }
    fn find_block_light_sources(&self, consumer: &mut dyn FnMut(BlockPos, BlockState)) {
        find_sources(&self.access, consumer);
    }
    fn get_sky_light_sources(&self) -> &ChunkSkyLightSources {
        &self.access.sky_light_sources
    }
}

impl LightChunk for LevelChunk {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        LevelChunk::get_block_state(self, pos)
    }
    fn find_block_light_sources(&self, consumer: &mut dyn FnMut(BlockPos, BlockState)) {
        find_sources(&self.access, consumer);
    }
    fn get_sky_light_sources(&self) -> &ChunkSkyLightSources {
        &self.access.sky_light_sources
    }
}

/// `LightChunkGetter` minus `onLightUpdate` (the engine reports changed sections through
/// [`crate::LevelLightEngine::take_light_updates`]).
pub trait LightChunkGetter {
    /// `getChunkForLighting(x, z)`: `None` for chunks that are not (yet) available.
    fn get_chunk_for_lighting(&self, x: i32, z: i32) -> Option<&dyn LightChunk>;
}

/// A plain map of chunks as a getter.
#[derive(Default)]
pub struct ChunkMapGetter<C> {
    pub chunks: HashMap<i64, C>,
}

impl<C: LightChunk> ChunkMapGetter<C> {
    pub fn new() -> Self {
        ChunkMapGetter {
            chunks: HashMap::new(),
        }
    }

    pub fn insert(&mut self, pos: ChunkPos, chunk: C) -> Option<C> {
        self.chunks.insert(pos.pack(), chunk)
    }
}

impl<C: LightChunk> LightChunkGetter for ChunkMapGetter<C> {
    fn get_chunk_for_lighting(&self, x: i32, z: i32) -> Option<&dyn LightChunk> {
        self.chunks
            .get(&ChunkPos::pack_xz(x, z))
            .map(|c| c as &dyn LightChunk)
    }
}

/// `LevelHeightAccessor` of the dimension, re-exported for convenience.
pub type LevelHeightAccessor = LevelHeight;
