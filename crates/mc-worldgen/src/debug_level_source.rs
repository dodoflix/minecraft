//! `DebugLevelSource`: the "debug all block states" world. `buildTerrain` is empty; `applyBiomeDecoration` writes a
//! barrier floor and the block-state grid.

use std::sync::LazyLock;

use mc_core::math::mth;
use mc_core::pos::{BlockPos, ChunkPos, SectionPos};
use mc_data::Block;
use mc_data::block::BlockState;
use mc_world::palette::BiomeId;

use std::sync::Arc;

use mc_world::chunk::{LevelHeight, ProtoChunk};
use mc_world::heightmap::HeightmapType;

use crate::biome::{BiomeGenerationSettings, BiomeRegistry, BiomeSource, FixedBiomeSource};
use crate::generator::{BuildTerrainContext, ChunkGenerator, NoiseColumn};
use crate::random_state::RandomState;

pub const HEIGHT: i32 = 70;
pub const BARRIER_HEIGHT: i32 = 60;

/// `ALL_BLOCKS`: every state of every block in registry order (`getPossibleStates` is state-id order).
static ALL_BLOCKS: LazyLock<Vec<BlockState>> =
    LazyLock::new(|| Block::iter().flat_map(|b| b.states()).collect());

/// `GRID_WIDTH = Mth.ceil(Mth.sqrt(ALL_BLOCKS.size()))`.
static GRID_WIDTH: LazyLock<i32> =
    LazyLock::new(|| mth::ceil_f32(mth::sqrt(ALL_BLOCKS.len() as f32)));

/// `GRID_HEIGHT = Mth.ceil((float) ALL_BLOCKS.size() / GRID_WIDTH)`.
static GRID_HEIGHT: LazyLock<i32> =
    LazyLock::new(|| mth::ceil_f32(ALL_BLOCKS.len() as f32 / *GRID_WIDTH as f32));

/// `ALL_BLOCKS`.
pub fn all_blocks() -> &'static [BlockState] {
    &ALL_BLOCKS
}

pub fn grid_width() -> i32 {
    *GRID_WIDTH
}

pub fn grid_height() -> i32 {
    *GRID_HEIGHT
}

/// `DebugLevelSource.getBlockStateFor(worldX, worldZ)`.
pub fn get_block_state_for(world_x: i32, world_z: i32) -> BlockState {
    let mut state = Block::AIR.default_state();
    if world_x > 0 && world_z > 0 && world_x % 2 != 0 && world_z % 2 != 0 {
        let world_x = world_x / 2;
        let world_z = world_z / 2;
        if world_x <= *GRID_WIDTH && world_z <= *GRID_HEIGHT {
            let index = mth::abs_i32(world_x.wrapping_mul(*GRID_WIDTH).wrapping_add(world_z));
            if (index as usize) < ALL_BLOCKS.len() {
                state = ALL_BLOCKS[index as usize];
            }
        }
    }
    state
}

/// The generator. The chunk-building methods are the `ChunkGenerator` impl (see `generator`); the decoration step
/// needs a level, so it is exposed as the list of block writes ([`Self::decoration_writes`]).
pub struct DebugLevelSource {
    biome_source: FixedBiomeSource,
    biomes: Arc<BiomeRegistry>,
}

impl DebugLevelSource {
    /// `new DebugLevelSource(plains)`.
    pub fn new(plains: BiomeId, biomes: Arc<BiomeRegistry>) -> DebugLevelSource {
        DebugLevelSource {
            biome_source: FixedBiomeSource::new(plains),
            biomes,
        }
    }

    /// `applyBiomeDecoration`: the `level.setBlock(pos, state, 2)` calls for `chunk_pos`, in vanilla order
    /// (x outer, z inner; per column the barrier at y=60 first, then the grid state at y=70).
    pub fn decoration_writes(&self, chunk_pos: ChunkPos) -> Vec<(BlockPos, BlockState)> {
        let barrier = Block::BARRIER.default_state();
        let mut writes = Vec::with_capacity(16 * 16 * 2);
        for x in 0..16 {
            for z in 0..16 {
                let world_x = SectionPos::section_to_block_coord_offset(chunk_pos.x, x);
                let world_z = SectionPos::section_to_block_coord_offset(chunk_pos.z, z);
                writes.push((BlockPos::new(world_x, BARRIER_HEIGHT, world_z), barrier));
                writes.push((
                    BlockPos::new(world_x, HEIGHT, world_z),
                    get_block_state_for(world_x, world_z),
                ));
            }
        }
        writes
    }
}

impl ChunkGenerator for DebugLevelSource {
    fn biome_source(&self) -> &dyn BiomeSource {
        &self.biome_source
    }

    fn get_biome_generation_settings(&self, biome: BiomeId) -> &BiomeGenerationSettings {
        self.biomes.get(biome).generation_settings()
    }

    fn build_terrain(&self, _chunk: &mut ProtoChunk, _ctx: &BuildTerrainContext) {}

    fn get_base_height(
        &self,
        _x: i32,
        _z: i32,
        _ty: HeightmapType,
        _height: LevelHeight,
        _random_state: &Arc<RandomState>,
    ) -> i32 {
        0
    }

    fn get_base_column(
        &self,
        _x: i32,
        _z: i32,
        _height: LevelHeight,
        _random_state: &Arc<RandomState>,
    ) -> NoiseColumn {
        NoiseColumn::new(0, Vec::new())
    }

    fn get_min_y(&self) -> i32 {
        0
    }

    fn get_gen_depth(&self) -> i32 {
        384
    }

    fn get_sea_level(&self) -> i32 {
        63
    }
}
