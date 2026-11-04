//! `GenEnv`: everything a level's generation needs besides the chunks — vanilla's `ChunkGenerator` as seen from
//! features/placements/structures (`chunkGenerator.getSeaLevel()`, `getBaseHeight(..)`, `getBiomeGenerationSettings(..)`,
//! `getBiomeSource()`), the `RandomState`, the registries and the seed. One per dimension, shared by all worldgen threads.

use super::registries::GenRegistries;
use super::structure::registries::{StartCache, StructureRegistries};
use mc_core::ident::Identifier;
use mc_core::pos::BlockPos;
use mc_world::chunk::LevelHeight;
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;
use mc_worldgen::biome::{BiomeGenerationSettings, BiomeRegistry, BiomeSource};
use mc_worldgen::generator::{ChunkGenerator, NoiseColumn};
use mc_worldgen::random_state::RandomState;
use mc_worldgen::world_generation_context::WorldGenerationContext;
use std::sync::Arc;

pub struct GenEnv {
    pub dimension: Identifier,
    /// `level.getSeed()`.
    pub seed: i64,
    pub generator: Arc<dyn ChunkGenerator>,
    pub random_state: Arc<RandomState>,
    pub biomes: Arc<BiomeRegistry>,
    pub registries: Arc<GenRegistries>,
    /// The `worldgen/structure` registry.
    pub structures: Arc<StructureRegistries>,
    /// Parsed structure starts of the level's chunks (see [`StartCache`]).
    pub start_cache: StartCache,
    /// `WorldOptions.generateStructures()`.
    pub generate_structures: bool,
    pub level_height: LevelHeight,
}

impl GenEnv {
    /// `ChunkGenerator.getBiomeSource()`.
    pub fn biome_source(&self) -> &dyn BiomeSource {
        self.generator.biome_source()
    }

    pub fn get_sea_level(&self) -> i32 {
        self.generator.get_sea_level()
    }

    pub fn get_gen_depth(&self) -> i32 {
        self.generator.get_gen_depth()
    }

    pub fn get_min_y(&self) -> i32 {
        self.generator.get_min_y()
    }

    /// `getBiomeGenerationSettings(biome)`.
    pub fn get_biome_generation_settings(&self, biome: BiomeId) -> &BiomeGenerationSettings {
        self.generator.get_biome_generation_settings(biome)
    }

    /// `getBaseHeight(x, z, type, heightAccessor, randomState)`.
    pub fn get_base_height(&self, x: i32, z: i32, ty: HeightmapType, height: LevelHeight) -> i32 {
        self.generator.get_base_height(x, z, ty, height, &self.random_state)
    }

    /// `getBaseColumn(x, z, heightAccessor, randomState)`.
    pub fn get_base_column(&self, x: i32, z: i32, height: LevelHeight) -> NoiseColumn {
        self.generator.get_base_column(x, z, height, &self.random_state)
    }

    /// `new WorldGenerationContext(generator, heightAccessor)`.
    pub fn world_generation_context(&self, height: LevelHeight) -> WorldGenerationContext {
        WorldGenerationContext::new(self.get_min_y(), self.get_gen_depth(), self.get_sea_level(), height)
    }

    pub fn first_pos_of_chunk(&self, chunk_x: i32, chunk_z: i32) -> BlockPos {
        BlockPos::new(chunk_x << 4, self.level_height.min_y, chunk_z << 4)
    }
}
