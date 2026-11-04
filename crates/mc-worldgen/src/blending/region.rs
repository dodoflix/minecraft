//! What `Blender` / `BlendingData` use of `WorldGenRegion` and `ChunkAccess`, as traits so `mc-game` can implement them
//! over its region/chunk holders. [`ProtoChunk`] and [`LevelChunk`] implement the chunk side.

use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::fluid::FluidState;
use mc_data::block::BlockState;
use mc_world::blending_data::BlendingData;
use mc_world::chunk::{LevelChunk, ProtoChunk};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;
use mc_world::status::ChunkStatus;

/// The read-only chunk queries of `BlendingData.calculateData`.
pub trait ChunkView {
    /// `getBlockState(pos)` (`VOID_AIR` outside the build height).
    fn get_block_state(&self, pos: BlockPos) -> BlockState;
    /// `getFluidState(pos)`.
    fn get_fluid_state(&self, pos: BlockPos) -> FluidState;
    /// `hasPrimedHeightmap(type)`.
    fn has_primed_heightmap(&self, ty: HeightmapType) -> bool;
    /// `getHeight(type, x, z)` of an already primed heightmap.
    fn get_height(&self, ty: HeightmapType, x: i32, z: i32) -> i32;
    /// `getNoiseBiome(quartX, quartY, quartZ)`.
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId;
}

/// A chunk of a [`BlendingRegion`] (`ChunkAccess`).
pub trait BlendingChunk: ChunkView {
    /// `isOldNoiseGeneration()` (`getBlendingData() != null`).
    fn is_old_noise_generation(&self) -> bool;
    /// `getHighestGeneratedStatus()`.
    fn get_highest_generated_status(&self) -> ChunkStatus;
    /// `getBlendingData()`.
    fn blending_data(&self) -> Option<&BlendingData>;
    /// Runs `f` with the chunk's blending data (`calculateData` mutates it in place) and a view of the rest of the
    /// chunk. Does nothing without blending data.
    fn update_blending_data(&mut self, f: &mut dyn FnMut(&mut BlendingData, &dyn ChunkView));
    /// `getHeight(type, x, z)`, priming the heightmap when absent.
    fn get_height_primed(&mut self, ty: HeightmapType, x: i32, z: i32) -> i32;
    /// `markPosForPostProcessing(pos)`.
    fn mark_pos_for_post_processing(&mut self, pos: BlockPos);
}

/// The part of `WorldGenRegion` that `Blender.of` / `generateBorderTicks` use. Takes `&self` like the vanilla
/// region, whose chunks are mutable behind the cache: implementations use interior mutability.
pub trait BlendingRegion {
    /// `getCenter()`.
    fn get_center(&self) -> ChunkPos;
    /// `isOldChunkAround(pos, range)` (`ChunkMap.isOldChunkAround`).
    fn is_old_chunk_around(&self, pos: ChunkPos, range: i32) -> bool;
    /// `getChunk(chunkX, chunkZ)`: runs `f` with the (within the region's reach, else vanilla throws) chunk.
    fn with_chunk(&self, chunk_x: i32, chunk_z: i32, f: &mut dyn FnMut(&mut dyn BlendingChunk));
}

impl dyn BlendingRegion + '_ {
    /// `with_chunk` returning a value.
    pub fn read_chunk<R>(&self, chunk_x: i32, chunk_z: i32, f: impl FnOnce(&mut dyn BlendingChunk) -> R) -> R {
        let mut f = Some(f);
        let mut result = None;
        self.with_chunk(chunk_x, chunk_z, &mut |chunk| {
            if let Some(f) = f.take() {
                result = Some(f(chunk));
            }
        });
        result.expect("BlendingRegion::with_chunk did not run the callback")
    }
}

impl ChunkView for ProtoChunk {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        ProtoChunk::get_block_state(self, pos)
    }
    fn get_fluid_state(&self, pos: BlockPos) -> FluidState {
        ProtoChunk::get_fluid_state(self, pos)
    }
    fn has_primed_heightmap(&self, ty: HeightmapType) -> bool {
        self.access.has_primed_heightmap(ty)
    }
    fn get_height(&self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        self.access.heightmaps[&ty].get_first_available(x & 15, z & 15) - 1
    }
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self.access.get_noise_biome(quart_x, quart_y, quart_z)
    }
}

impl ChunkView for LevelChunk {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        LevelChunk::get_block_state(self, pos)
    }
    fn get_fluid_state(&self, pos: BlockPos) -> FluidState {
        LevelChunk::get_fluid_state(self, pos.x(), pos.y(), pos.z())
    }
    fn has_primed_heightmap(&self, ty: HeightmapType) -> bool {
        self.access.has_primed_heightmap(ty)
    }
    fn get_height(&self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        self.access.heightmaps[&ty].get_first_available(x & 15, z & 15) - 1
    }
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self.access.get_noise_biome(quart_x, quart_y, quart_z)
    }
}

impl BlendingChunk for ProtoChunk {
    fn is_old_noise_generation(&self) -> bool {
        self.access.is_old_noise_generation()
    }
    fn get_highest_generated_status(&self) -> ChunkStatus {
        ProtoChunk::get_highest_generated_status(self)
    }
    fn blending_data(&self) -> Option<&BlendingData> {
        self.access.blending_data.as_ref()
    }
    fn update_blending_data(&mut self, f: &mut dyn FnMut(&mut BlendingData, &dyn ChunkView)) {
        if let Some(mut data) = self.access.blending_data.take() {
            f(&mut data, &*self);
            self.access.blending_data = Some(data);
        }
    }
    fn get_height_primed(&mut self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        self.access.get_height(true, ty, x, z)
    }
    fn mark_pos_for_post_processing(&mut self, pos: BlockPos) {
        ProtoChunk::mark_pos_for_post_processing(self, pos)
    }
}

impl BlendingChunk for LevelChunk {
    fn is_old_noise_generation(&self) -> bool {
        self.access.is_old_noise_generation()
    }
    fn get_highest_generated_status(&self) -> ChunkStatus {
        ChunkStatus::Full
    }
    fn blending_data(&self) -> Option<&BlendingData> {
        self.access.blending_data.as_ref()
    }
    fn update_blending_data(&mut self, f: &mut dyn FnMut(&mut BlendingData, &dyn ChunkView)) {
        if let Some(mut data) = self.access.blending_data.take() {
            f(&mut data, &*self);
            self.access.blending_data = Some(data);
        }
    }
    fn get_height_primed(&mut self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        self.access.get_height(false, ty, x, z)
    }
    fn mark_pos_for_post_processing(&mut self, pos: BlockPos) {
        if self.access.level_height.is_inside_build_height(pos.y()) {
            let index = self.access.level_height.section_index(pos.y()) as usize;
            self.access.post_processing[index]
                .get_or_insert_with(Vec::new)
                .push(mc_world::chunk::pack_offset_coordinates(pos));
        }
    }
}
