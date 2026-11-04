//! Adapter from `mc_worldgen`'s state-only `ChunkGenerator` (+ its `RandomState`) to the chunk map's
//! [`ChunkGeneratorBackend`]: biome creation and terrain building over a [`WorldGenRegion`].

use super::world_gen::{ChunkGeneratorBackend, WorldGenRegion};
use mc_core::ident::Identifier;
use mc_core::pos::{ChunkPos, SectionPos};
use mc_world::chunk::ProtoChunk;
use mc_world::palette::BiomeId;
use mc_worldgen::biome::{BiomeManager, BiomeResolver};
use mc_worldgen::blending::Blender;
use mc_worldgen::generator::{BuildTerrainContext, ChunkGenerator};
use mc_worldgen::random_state::RandomState;
use std::sync::Arc;

pub struct NoiseWorldGenerator {
    generator: Arc<dyn ChunkGenerator>,
    random_state: Arc<RandomState>,
    seed: i64,
    type_name: Option<Identifier>,
}

impl NoiseWorldGenerator {
    /// `type_name`: the generator's `type` id for the data fixer context (`getTypeNameForDataFixer`).
    pub fn new(generator: Arc<dyn ChunkGenerator>, random_state: Arc<RandomState>, seed: i64, type_name: Option<Identifier>) -> Self {
        Self { generator, random_state, seed, type_name }
    }

    pub fn generator(&self) -> &Arc<dyn ChunkGenerator> {
        &self.generator
    }

    pub fn random_state(&self) -> &Arc<RandomState> {
        &self.random_state
    }
}

/// `WorldGenRegion.getNoiseBiome`: the biome stored in the region's chunks.
struct RegionBiomes<'a>(&'a WorldGenRegion);

impl BiomeResolver for RegionBiomes<'_> {
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        let cx = SectionPos::block_to_section_coord_i32(quart_x << 2);
        let cz = SectionPos::block_to_section_coord_i32(quart_z << 2);
        match self.0.get_chunk(cx, cz) {
            Some(chunk) => chunk.access.get_noise_biome(quart_x, quart_y, quart_z),
            None => panic!("Biome requested outside of the generation region: chunk {cx},{cz}"),
        }
    }
}

impl ChunkGeneratorBackend for NoiseWorldGenerator {
    fn type_name_for_data_fixer(&self) -> Option<Identifier> {
        self.type_name.clone()
    }

    fn get_origin(&self) -> ChunkPos {
        self.generator.get_origin(&self.random_state)
    }

    fn get_spawn_height(&self, height: mc_world::chunk::LevelHeight) -> i32 {
        self.generator.get_spawn_height(height)
    }

    fn get_base_height(&self, x: i32, z: i32, ty: mc_world::heightmap::HeightmapType, height: mc_world::chunk::LevelHeight) -> i32 {
        self.generator.get_base_height(x, z, ty, height, &self.random_state)
    }

    fn create_biomes(&self, chunk: &mut ProtoChunk, _region: &WorldGenRegion) {
        // `Blender.of(region)` is empty unless old-version blending data is around (not supported).
        self.generator.create_biomes(&self.random_state, &Blender::empty(), chunk);
    }

    fn build_terrain(&self, chunk: &mut ProtoChunk, region: &WorldGenRegion) {
        // `collectPossibleBiomes(region, 1)`: biomes of the 3x3 chunks (the center chunk is lent separately).
        let center = chunk.pos();
        let mut possible: Vec<BiomeId> = Vec::new();
        let mut collect = |b: BiomeId| {
            if !possible.contains(&b) {
                possible.push(b);
            }
        };
        for dz in -1..=1 {
            for dx in -1..=1 {
                let pos = ChunkPos::new(center.x + dx, center.z + dz);
                if pos == center {
                    chunk.access.collect_biomes_in_palette(&mut collect);
                } else if let Some(c) = region.get_chunk(pos.x, pos.z) {
                    c.access.collect_biomes_in_palette(&mut collect);
                }
            }
        }
        // The center chunk is mutated by `build_terrain`, so its biomes are read through a snapshot (at this stage the
        // sections are still empty, the clone is cheap).
        let snapshot = chunk.clone();
        let resolver = RegionBiomesWithCenter { region, center, center_chunk: &snapshot };
        let biome_manager = BiomeManager::new(&resolver, BiomeManager::obfuscate_seed(self.seed));
        let ctx = BuildTerrainContext {
            random_state: &self.random_state,
            blender: &Blender::empty(),
            structure_starts: &[],
            biome_manager: &biome_manager,
            possible_biomes: Some(&possible),
            carver_biome_cache: None,
        };
        self.generator.build_terrain(chunk, &ctx);
    }
}

/// Region biomes where the center chunk is the lent chunk itself.
struct RegionBiomesWithCenter<'a> {
    region: &'a WorldGenRegion,
    center: ChunkPos,
    center_chunk: &'a ProtoChunk,
}

impl BiomeResolver for RegionBiomesWithCenter<'_> {
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        let cx = SectionPos::block_to_section_coord_i32(quart_x << 2);
        let cz = SectionPos::block_to_section_coord_i32(quart_z << 2);
        if ChunkPos::new(cx, cz) == self.center {
            return self.center_chunk.access.get_noise_biome(quart_x, quart_y, quart_z);
        }
        RegionBiomes(self.region).get_noise_biome(quart_x, quart_y, quart_z)
    }
}
