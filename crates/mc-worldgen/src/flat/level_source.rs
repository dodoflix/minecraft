//! `FlatLevelSource`: the flat chunk generator (layer fill, fixed biome, fixed -63 sea level / 0 min y / 384 depth).

use std::sync::{Arc, OnceLock, RwLock, RwLockReadGuard};

use mc_core::pos::BlockPos;
use mc_data::Block;
use mc_data::block::BlockState;
use mc_world::chunk::{LevelHeight, ProtoChunk, SectionsView};
use mc_world::heightmap::{Heightmap, HeightmapType};
use mc_world::palette::BiomeId;

use super::settings::FlatLevelGeneratorSettings;
use crate::biome::{BiomeGenerationSettings, BiomeRegistry, BiomeSource, FixedBiomeSource};
use crate::generator::{BuildTerrainContext, ChunkGenerator, NoiseColumn};
use crate::random_state::RandomState;

pub struct FlatLevelSource {
    /// Shared mutable like vanilla's: `adjust_generation_settings` nulls non-opaque layers in place.
    settings: RwLock<FlatLevelGeneratorSettings>,
    biome_source: FixedBiomeSource,
    biomes: Arc<BiomeRegistry>,
    /// `Util.memoize(settings::adjustGenerationSettings)` for the flat biome (other biomes pass straight through).
    flat_biome_settings: OnceLock<BiomeGenerationSettings>,
}

impl FlatLevelSource {
    /// `new FlatLevelSource(settings)`.
    pub fn new(
        settings: FlatLevelGeneratorSettings,
        biomes: Arc<BiomeRegistry>,
    ) -> FlatLevelSource {
        FlatLevelSource {
            biome_source: FixedBiomeSource::new(settings.get_biome()),
            settings: RwLock::new(settings),
            biomes,
            flat_biome_settings: OnceLock::new(),
        }
    }

    /// `settings()`.
    pub fn settings(&self) -> RwLockReadGuard<'_, FlatLevelGeneratorSettings> {
        self.settings.read().unwrap_or_else(|p| p.into_inner())
    }
}

impl ChunkGenerator for FlatLevelSource {
    fn biome_source(&self) -> &dyn BiomeSource {
        &self.biome_source
    }

    fn get_biome_generation_settings(&self, biome: BiomeId) -> &BiomeGenerationSettings {
        if biome != self.settings().get_biome() {
            return self.biomes.get(biome).generation_settings();
        }
        self.flat_biome_settings.get_or_init(|| {
            let mut settings = self.settings.write().unwrap_or_else(|p| p.into_inner());
            settings
                .adjust_generation_settings(&self.biomes, biome)
                .into_owned()
        })
    }

    fn get_spawn_height(&self, height: LevelHeight) -> i32 {
        let layers = self.settings().get_layers().len() as i32;
        height.min_y.wrapping_add(height.height.min(layers))
    }

    fn build_terrain(&self, chunk: &mut ProtoChunk, _ctx: &BuildTerrainContext) {
        let settings = self.settings();
        let layers = settings.get_layers();
        let min_y = chunk.access.min_y();
        // getOrCreateHeightmapUnprimed(OCEAN_FLOOR_WG / WORLD_SURFACE_WG)
        for ty in [HeightmapType::OceanFloorWg, HeightmapType::WorldSurfaceWg] {
            chunk.access.get_or_create_heightmap_unprimed(ty);
        }
        let count = (chunk.access.height() as i64).min(layers.len() as i64) as i32;
        for layer_index in 0..count.max(0) {
            let Some(block_state) = layers[layer_index as usize] else {
                continue;
            };
            let y = min_y.wrapping_add(layer_index);
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set_block_state(BlockPos::new(x, y, z), block_state, None);
                    let access = &mut chunk.access;
                    let view = SectionsView {
                        level_height: access.level_height,
                        sections: &access.sections,
                        void_air_outside: true,
                    };
                    for ty in [HeightmapType::OceanFloorWg, HeightmapType::WorldSurfaceWg] {
                        let heightmap: &mut Heightmap = access
                            .heightmaps
                            .get_mut(&ty)
                            .expect("created above and never removed");
                        heightmap.update(x, y, z, block_state, &view);
                    }
                }
            }
        }
    }

    fn get_base_height(
        &self,
        _x: i32,
        _z: i32,
        ty: HeightmapType,
        height: LevelHeight,
        _random_state: &Arc<RandomState>,
    ) -> i32 {
        let settings = self.settings();
        let layers = settings.get_layers();
        let mut layer_index = (layers.len() as i32 - 1).min(height.max_y());
        while layer_index >= 0 {
            if let Some(state) = layers[layer_index as usize]
                && ty.is_opaque(state)
            {
                return height.min_y + layer_index + 1;
            }
            layer_index -= 1;
        }
        height.min_y
    }

    fn get_base_column(
        &self,
        _x: i32,
        _z: i32,
        height: LevelHeight,
        _random_state: &Arc<RandomState>,
    ) -> NoiseColumn {
        let air: BlockState = Block::AIR.default_state();
        let column = self
            .settings()
            .get_layers()
            .iter()
            .take(height.height.max(0) as usize)
            .map(|s| s.unwrap_or(air))
            .collect();
        NoiseColumn::new(height.min_y, column)
    }

    fn get_min_y(&self) -> i32 {
        0
    }

    fn get_gen_depth(&self) -> i32 {
        384
    }

    fn get_sea_level(&self) -> i32 {
        -63
    }
}
