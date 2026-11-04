//! `net.minecraft.world.level.chunk.ChunkGenerator` (the state-only part): the trait every generator
//! (noise, flat, debug) implements, plus `NoiseColumn`/`BlockColumn`.

use std::sync::Arc;

use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::quart_pos;
use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;

use crate::beardifier::StructureStartInfo;
use crate::biome::{BiomeGenerationSettings, BiomeManager, BiomeSource};
use crate::blending::Blender;
use crate::densityfunction::SamplerContext;
use crate::random_state::RandomState;

mod noise_based;
mod noise_column;

pub use noise_based::NoiseBasedChunkGenerator;
pub use noise_column::{BlockColumn, NoiseColumn};

/// `ChunkAccess.carverBiome(supplier)` as used by `NoiseBasedChunkGenerator.generateCarvers` with a
/// `carverBiomeRegion`: the biome used for the carver settings of a source chunk is computed once per chunk and
/// remembered by the implementor (interior mutability).
pub trait CarverBiomeCache {
    /// Returns the cached biome of `chunk_pos`, calling `compute` (and caching its result) on a miss.
    fn carver_biome(&self, chunk_pos: ChunkPos, compute: &mut dyn FnMut() -> BiomeId) -> BiomeId;
}

/// The arguments of `ChunkGenerator.buildTerrain` besides the chunk itself.
pub struct BuildTerrainContext<'a> {
    pub random_state: &'a Arc<RandomState>,
    pub blender: &'a Blender,
    /// `StructureManager.startsForStructure(chunkX, chunkZ, ...)`: the starts reaching the chunk, for the beardifier
    /// (`Beardifier.forStructuresInChunk`). Starts with `terrainAdaptation() == NONE` may be included.
    pub structure_starts: &'a [StructureStartInfo],
    pub biome_manager: &'a BiomeManager<'a>,
    /// `possibleBiomes` handed to the surface system (`None` = all biomes of the generator's biome source).
    pub possible_biomes: Option<&'a [BiomeId]>,
    /// `carverBiomeRegion`: per-chunk cache of the carver biome of the 17x17 source chunks.
    pub carver_biome_cache: Option<&'a dyn CarverBiomeCache>,
}

/// Boxed biome function `(quartX, quartY, quartZ) -> biome` (what [`Blender::get_biome_resolver`] returns).
pub type BiomeFn<'a> = Box<dyn Fn(i32, i32, i32) -> BiomeId + 'a>;

/// `ChunkGenerator`.
pub trait ChunkGenerator: Send + Sync {
    /// `getBiomeSource()`.
    fn biome_source(&self) -> &dyn BiomeSource;

    /// `getBiomeGenerationSettings(biome)` (flat generators adjust the biome's settings).
    fn get_biome_generation_settings(&self, biome: BiomeId) -> &BiomeGenerationSettings;

    /// `getOrigin(randomState)`.
    fn get_origin(&self, _random_state: &Arc<RandomState>) -> ChunkPos {
        ChunkPos::ZERO
    }

    /// `decorateBiomeResolver(blender, protoChunk, biomeResolver)`.
    fn decorate_biome_resolver<'a>(&'a self, _blender: &'a Blender, _chunk: &ProtoChunk, biome_resolver: BiomeFn<'a>) -> BiomeFn<'a> {
        biome_resolver
    }

    /// `createBiomes(randomState, blender, structureManager, protoChunk)` / `doCreateBiomes`.
    fn create_biomes(&self, random_state: &Arc<RandomState>, blender: &Blender, chunk: &mut ProtoChunk) {
        let climate_sampler = random_state.create_climate_sampler(SamplerContext::builder().enable_caches().build());
        let pos = chunk.pos();
        let biome_resolver = self.biome_source().create_resolver_for_chunk(
            climate_sampler,
            quart_pos::from_block(pos.get_min_block_x()),
            quart_pos::from_block(chunk.access.min_y()),
            quart_pos::from_block(pos.get_min_block_z()),
            quart_pos::from_block(16),
            quart_pos::from_block(chunk.access.height()),
            quart_pos::from_block(16),
        );
        let resolver: BiomeFn<'_> = Box::new(move |x, y, z| biome_resolver.get_noise_biome(x, y, z));
        let decorated = self.decorate_biome_resolver(blender, chunk, resolver);
        let height_for_generation = chunk.get_height_accessor_for_generation();
        chunk.access.fill_biomes_from_noise(height_for_generation, |x, y, z| decorated(x, y, z));
    }

    /// `buildTerrain(chunk, blender, randomState, structureManager, biomeManager, carverBiomeRegion, possibleBiomes)`:
    /// fills the terrain, builds the surface and carves.
    fn build_terrain(&self, chunk: &mut ProtoChunk, ctx: &BuildTerrainContext<'_>);

    /// `getGenDepth()`.
    fn get_gen_depth(&self) -> i32;

    /// `getSeaLevel()`.
    fn get_sea_level(&self) -> i32;

    /// `getMinY()`.
    fn get_min_y(&self) -> i32;

    /// `getSpawnHeight(heightAccessor)`.
    fn get_spawn_height(&self, _height_accessor: LevelHeight) -> i32 {
        64
    }

    /// `getBaseHeight(x, z, type, heightAccessor, randomState)`.
    fn get_base_height(
        &self,
        x: i32,
        z: i32,
        ty: HeightmapType,
        height_accessor: LevelHeight,
        random_state: &Arc<RandomState>,
    ) -> i32;

    /// `getBaseColumn(x, z, heightAccessor, randomState)`.
    fn get_base_column(&self, x: i32, z: i32, height_accessor: LevelHeight, random_state: &Arc<RandomState>) -> NoiseColumn;

    /// `getFirstFreeHeight(x, z, type, heightAccessor, randomState)`.
    fn get_first_free_height(
        &self,
        x: i32,
        z: i32,
        ty: HeightmapType,
        height_accessor: LevelHeight,
        random_state: &Arc<RandomState>,
    ) -> i32 {
        self.get_base_height(x, z, ty, height_accessor, random_state)
    }

    /// `getFirstOccupiedHeight(x, z, type, heightAccessor, randomState)`.
    fn get_first_occupied_height(
        &self,
        x: i32,
        z: i32,
        ty: HeightmapType,
        height_accessor: LevelHeight,
        random_state: &Arc<RandomState>,
    ) -> i32 {
        self.get_base_height(x, z, ty, height_accessor, random_state) - 1
    }
}
