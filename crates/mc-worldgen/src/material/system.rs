//! `MaterialSystem`: builds the surface of a chunk by running a compiled [`MaterialRule`] down every
//! column, plus the eroded-badlands / frozen-ocean extensions and the clay bands.

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::math::java::{min_f64, round_f32};
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::{PositionalRandomFactory, RandomSource};
use mc_data::block::{Block, BlockState};
use mc_world::chunk::ProtoChunk;
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;

use super::context::MaterialRuleContext;
use super::rule::MaterialRule;
use crate::biome::{BiomeManager, BiomeRegistry};
use crate::data::LoadError;
use crate::densityfunction::{DensityFunction, DensityVolume};
use crate::noise_settings::DIMENSION_MIN_Y;
use crate::noises::{self, names};
use crate::random_state::{DensitySamplerSet, RandomState};
use crate::synth::Noise;
use crate::world_generation_context::WorldGenerationContext;

/// `DimensionType.WAY_BELOW_MIN_Y`.
const WAY_BELOW_MIN_Y: i32 = DIMENSION_MIN_Y << 4;

pub struct MaterialSystem {
    default_block: BlockState,
    sea_level: i32,
    preliminary_surface_function: DensityFunction,
    clay_bands: Vec<BlockState>,
    clay_bands_offset_noise: Arc<dyn Noise>,
    badlands_pillar_noise: Arc<dyn Noise>,
    badlands_pillar_roof_noise: Arc<dyn Noise>,
    badlands_surface_noise: Arc<dyn Noise>,
    iceberg_pillar_noise: Arc<dyn Noise>,
    iceberg_pillar_roof_noise: Arc<dyn Noise>,
    iceberg_surface_noise: Arc<dyn Noise>,
    noise_random: Arc<dyn PositionalRandomFactory>,
    surface_noise: Arc<dyn Noise>,
    surface_secondary_noise: Arc<dyn Noise>,
}

/// A chunk's `BlockColumn` at a fixed x/z.
struct ChunkBlockColumn<'c> {
    chunk: &'c mut ProtoChunk,
    x: i32,
    z: i32,
    min_y: i32,
    max_y: i32,
}

impl ChunkBlockColumn<'_> {
    fn get_block(&self, y: i32) -> BlockState {
        self.chunk.get_block_state(BlockPos::new(self.x, y, self.z))
    }

    fn set_block(&mut self, y: i32, state: BlockState) {
        if y >= self.min_y && y <= self.max_y {
            let pos = BlockPos::new(self.x, y, self.z);
            self.chunk.set_block_state(pos, state, None);
            if !state.get_fluid_state().is_empty() {
                self.chunk.mark_pos_for_post_processing(pos);
            }
        }
    }
}

fn block(block: Block) -> BlockState {
    block.default_state()
}

impl MaterialSystem {
    /// `new MaterialSystem(randomState, defaultBlock, seaLevel, preliminarySurfaceFunction, noiseRandom)`.
    /// Fails if a noise the system needs is not in the noise registry.
    pub fn new(
        random_state: &RandomState,
        default_block: BlockState,
        sea_level: i32,
        preliminary_surface_function: DensityFunction,
        noise_random: Arc<dyn PositionalRandomFactory>,
    ) -> Result<MaterialSystem, LoadError> {
        let noise = |name: &str| random_state.get_or_create_noise(&noises::key(name));
        let clay_bands_offset_noise = noise(names::CLAY_BANDS_OFFSET)?;
        let clay_bands = generate_bands(&mut *noise_random.from_hash_of_identifier(
            &Identifier::with_default_namespace("clay_bands").expect("valid identifier"),
        ));
        Ok(MaterialSystem {
            default_block,
            sea_level,
            preliminary_surface_function,
            clay_bands,
            clay_bands_offset_noise,
            surface_noise: noise(names::SURFACE)?,
            surface_secondary_noise: noise(names::SURFACE_SECONDARY)?,
            badlands_pillar_noise: noise(names::BADLANDS_PILLAR)?,
            badlands_pillar_roof_noise: noise(names::BADLANDS_PILLAR_ROOF)?,
            badlands_surface_noise: noise(names::BADLANDS_SURFACE)?,
            iceberg_pillar_noise: noise(names::ICEBERG_PILLAR)?,
            iceberg_pillar_roof_noise: noise(names::ICEBERG_PILLAR_ROOF)?,
            iceberg_surface_noise: noise(names::ICEBERG_SURFACE)?,
            noise_random,
        })
    }

    /// `buildSurface(randomState, biomeManager, generationContext, protoChunk, noiseChunk, ruleSource, possibleBiomes)`.
    /// `volume` is `noiseChunk.volume()`, `samplers` is `noiseChunk.cachingSamplers()`; `biomes` resolves
    /// biome handles to their data (temperature checks).
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the vanilla `buildSurface` parameter list"
    )]
    pub fn build_surface(
        &self,
        random_state: &RandomState,
        biome_manager: &BiomeManager,
        generation_context: &WorldGenerationContext,
        biomes: &BiomeRegistry,
        proto_chunk: &mut ProtoChunk,
        volume: &DensityVolume,
        samplers: &DensitySamplerSet,
        rule_source: &MaterialRule,
        possible_biomes: Option<&[BiomeId]>,
    ) {
        let chunk_pos = proto_chunk.pos();
        let min_block_x = chunk_pos.get_min_block_x();
        let min_block_z = chunk_pos.get_min_block_z();
        let height_accessor = proto_chunk.get_height_accessor_for_generation();
        let min_y = height_accessor.min_y;
        let max_y = height_accessor.max_y();
        let highest_filled_section_index = proto_chunk.access.get_highest_filled_section_index();
        let max_block_y = proto_chunk
            .access
            .level_height
            .section_y_from_section_index(highest_filled_section_index)
            * 16
            + 15;
        let narrowed_volume = DensityVolume::with_unit_step(
            volume.size_x,
            (max_block_y - volume.min_block_y + 1).max(1),
            volume.size_z,
            volume.min_block_x,
            volume.min_block_y,
            volume.min_block_z,
        );
        let biome_getter = |pos: BlockPos| biome_manager.get_biome(pos);
        let mut context = MaterialRuleContext::new(
            self,
            random_state,
            narrowed_volume,
            samplers,
            &biome_getter,
            biomes,
            generation_context,
            possible_biomes,
        );
        let mut rule = rule_source.compile(&mut context);
        let eroded_badlands = biomes.id_of(&biome_key("eroded_badlands"));
        let frozen_ocean = biomes.id_of(&biome_key("frozen_ocean"));
        let deep_frozen_ocean = biomes.id_of(&biome_key("deep_frozen_ocean"));

        for x in 0..16 {
            for z in 0..16 {
                let block_x = min_block_x + x;
                let block_z = min_block_z + z;
                let starting_height =
                    proto_chunk
                        .access
                        .get_height(true, HeightmapType::WorldSurfaceWg, x, z)
                        + 1;
                let surface_biome =
                    biome_manager.get_biome(BlockPos::new(block_x, starting_height, block_z));
                let mut column = ChunkBlockColumn {
                    chunk: &mut *proto_chunk,
                    x: block_x,
                    z: block_z,
                    min_y,
                    max_y,
                };
                if Some(surface_biome) == eroded_badlands {
                    self.eroded_badlands_extension(&mut column, block_x, block_z, starting_height);
                }

                let height =
                    column
                        .chunk
                        .access
                        .get_height(true, HeightmapType::WorldSurfaceWg, x, z)
                        + 1;
                let gradient_x = get_surface_gradient_x(column.chunk, x, z);
                let gradient_z = get_surface_gradient_z(column.chunk, x, z);
                context.update_xz(block_x, block_z, gradient_x, gradient_z);
                let mut stone_above_depth = 0;
                let mut water_height = i32::MIN;
                let mut next_ceiling_stone_y = i32::MAX;
                let end_y = column.chunk.access.min_y();

                let mut y = height;
                while y >= end_y {
                    let old = column.get_block(y);
                    if old.is_air() {
                        stone_above_depth = 0;
                        water_height = i32::MIN;
                    } else if !old.get_fluid_state().is_empty() {
                        if water_height == i32::MIN {
                            water_height = y + 1;
                        }
                    } else {
                        if next_ceiling_stone_y >= y {
                            next_ceiling_stone_y = WAY_BELOW_MIN_Y;
                            let mut lookahead_y = y - 1;
                            while lookahead_y >= end_y - 1 {
                                let next_state = column.get_block(lookahead_y);
                                if !is_stone(next_state) {
                                    next_ceiling_stone_y = lookahead_y + 1;
                                    break;
                                }
                                lookahead_y -= 1;
                            }
                        }

                        stone_above_depth += 1;
                        let stone_below_depth = y - next_ceiling_stone_y + 1;
                        context.update_y(stone_above_depth, stone_below_depth, water_height, y);
                        if (min_y..=max_y).contains(&y)
                            && let Some(state) = rule.try_apply(&mut context, block_x, y, block_z)
                        {
                            column.set_block(y, state);
                        }
                    }
                    y -= 1;
                }

                if Some(surface_biome) == frozen_ocean || Some(surface_biome) == deep_frozen_ocean {
                    let min_surface_level = context.get_min_surface_level();
                    self.frozen_ocean_extension(
                        min_surface_level,
                        biomes,
                        surface_biome,
                        &mut column,
                        block_x,
                        block_z,
                        starting_height,
                    );
                }
            }
        }
    }

    /// `getSurfaceDepth(blockX, blockZ)`.
    pub fn get_surface_depth(&self, block_x: i32, block_z: i32) -> i32 {
        let noise_value = f64::from(self.surface_noise.get3(
            f64::from(block_x),
            0.0,
            f64::from(block_z),
        ));
        (noise_value * 2.75 + 3.0 + self.noise_random.at(block_x, 0, block_z).next_double() * 0.25)
            as i32
    }

    /// `getSurfaceSecondary(blockX, blockZ)`.
    pub fn get_surface_secondary(&self, block_x: i32, block_z: i32) -> f64 {
        f64::from(
            self.surface_secondary_noise
                .get3(f64::from(block_x), 0.0, f64::from(block_z)),
        )
    }

    /// `getSeaLevel()`.
    pub fn get_sea_level(&self) -> i32 {
        self.sea_level
    }

    /// `topMaterial(ruleSource, randomState, worldGenerationContext, biomeGetter, chunk, densitySamplers, pos, underFluid)`.
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the vanilla `topMaterial` parameter list"
    )]
    pub fn top_material(
        &self,
        rule_source: &MaterialRule,
        random_state: &RandomState,
        world_generation_context: &WorldGenerationContext,
        biomes: &BiomeRegistry,
        biome_getter: &dyn Fn(BlockPos) -> BiomeId,
        chunk: &mut ProtoChunk,
        density_samplers: &DensitySamplerSet,
        pos: BlockPos,
        under_fluid: bool,
    ) -> Option<BlockState> {
        let volume = DensityVolume::with_unit_step(1, 1, 1, pos.x(), pos.y(), pos.z());
        let mut context = MaterialRuleContext::new(
            self,
            random_state,
            volume,
            density_samplers,
            biome_getter,
            biomes,
            world_generation_context,
            None,
        );
        let mut rule = rule_source.compile(&mut context);
        let (block_x, block_y, block_z) = (pos.x(), pos.y(), pos.z());
        let gradient_x = get_surface_gradient_x(chunk, block_x & 15, block_z & 15);
        let gradient_z = get_surface_gradient_z(chunk, block_x & 15, block_z & 15);
        context.update_xz(block_x, block_z, gradient_x, gradient_z);
        context.update_y(
            1,
            1,
            if under_fluid { block_y + 1 } else { i32::MIN },
            block_y,
        );
        rule.try_apply(&mut context, block_x, block_y, block_z)
    }

    fn eroded_badlands_extension(
        &self,
        column: &mut ChunkBlockColumn,
        block_x: i32,
        block_z: i32,
        height: i32,
    ) {
        let (x, z) = (f64::from(block_x), f64::from(block_z));
        let pillar_buffer = min_f64(
            (f64::from(self.badlands_surface_noise.get3(x, 0.0, z)) * 8.25).abs(),
            f64::from(self.badlands_pillar_noise.get3(x * 0.2, 0.0, z * 0.2) * 15.0f32),
        );
        // NaN-aware like Java: only a NaN or positive value gets here.
        if !(pillar_buffer <= 0.0) {
            let pillar_floor = (f64::from(self.badlands_pillar_roof_noise.get3(
                x * 0.75,
                0.0,
                z * 0.75,
            )) * 1.5)
                .abs();
            let extension_top = 64.0
                + min_f64(
                    pillar_buffer * pillar_buffer * 2.5,
                    (pillar_floor * 50.0).ceil() + 24.0,
                );
            let start_y = mth::floor_f64(extension_top);
            if height <= start_y {
                let min_y = column.chunk.access.min_y();
                let mut y = start_y;
                while y >= min_y {
                    let old_state = column.get_block(y);
                    if old_state.block() == self.default_block.block() {
                        break;
                    }
                    if old_state.block() == Block::WATER {
                        return;
                    }
                    y -= 1;
                }

                let mut y = start_y;
                while y >= min_y && column.get_block(y).is_air() {
                    column.set_block(y, self.default_block);
                    y -= 1;
                }
            }
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the vanilla `frozenOceanExtension` parameter list"
    )]
    fn frozen_ocean_extension(
        &self,
        min_surface_level: i32,
        biomes: &BiomeRegistry,
        surface_biome: BiomeId,
        column: &mut ChunkBlockColumn,
        block_x: i32,
        block_z: i32,
        height: i32,
    ) {
        let (x, z) = (f64::from(block_x), f64::from(block_z));
        let iceberg = min_f64(
            (f64::from(self.iceberg_surface_noise.get3(x, 0.0, z)) * 8.25).abs(),
            f64::from(self.iceberg_pillar_noise.get3(x * 1.28, 0.0, z * 1.28) * 15.0f32),
        );
        if !(iceberg <= 1.8) {
            let iceberg_roof =
                (f64::from(self.iceberg_pillar_roof_noise.get3(x * 1.17, 0.0, z * 1.17)) * 1.5)
                    .abs();
            let mut top = min_f64(iceberg * iceberg * 1.2, (iceberg_roof * 40.0).ceil() + 14.0);
            if biomes
                .get(surface_biome)
                .should_melt_frozen_ocean_iceberg_slightly(
                    BlockPos::new(block_x, self.sea_level, block_z),
                    self.sea_level,
                )
            {
                top -= 2.0;
            }

            if !(top <= 2.0) {
                let sea_level = f64::from(self.sea_level);
                let extension_bottom = sea_level - top - 7.0;
                top += sea_level;
                let extension_top = top;
                let mut random = self.noise_random.at(block_x, 0, block_z);
                let max_snow_depth = 2 + random.next_int_bounded(4);
                let min_snow_height = self.sea_level + 18 + random.next_int_bounded(10);
                let mut snow_depth = 0;

                let mut y = height.max(top as i32 + 1);
                while y >= min_surface_level {
                    if (column.get_block(y).is_air()
                        && y < extension_top as i32
                        && random.next_double() > 0.01)
                        || (column.get_block(y).block() == Block::WATER
                            && y > extension_bottom as i32
                            && y < self.sea_level
                            && random.next_double() > 0.15)
                    {
                        if snow_depth <= max_snow_depth && y > min_snow_height {
                            column.set_block(y, block(Block::SNOW_BLOCK));
                            snow_depth += 1;
                        } else {
                            column.set_block(y, block(Block::PACKED_ICE));
                        }
                    }
                    y -= 1;
                }
            }
        }
    }

    /// `getBand(worldX, y, worldZ)`.
    pub fn get_band(&self, world_x: i32, y: i32, world_z: i32) -> BlockState {
        let offset = round_f32(
            self.clay_bands_offset_noise
                .get3(f64::from(world_x), 0.0, f64::from(world_z))
                * 4.0f32,
        );
        let length = self.clay_bands.len() as i32;
        self.clay_bands[((y.wrapping_add(offset).wrapping_add(length)) % length) as usize]
    }

    /// `preliminarySurfaceFunction()`.
    pub fn preliminary_surface_function(&self) -> &DensityFunction {
        &self.preliminary_surface_function
    }

    /// The generated clay bands (test access).
    pub fn clay_bands(&self) -> &[BlockState] {
        &self.clay_bands
    }
}

fn biome_key(path: &str) -> Identifier {
    Identifier::with_default_namespace(path).expect("valid biome key")
}

/// `isStone(state)`.
fn is_stone(state: BlockState) -> bool {
    !state.is_air() && state.get_fluid_state().is_empty()
}

/// `getSurfaceGradientX(protoChunk, x, z)`.
fn get_surface_gradient_x(chunk: &mut ProtoChunk, x: i32, z: i32) -> i32 {
    chunk
        .access
        .get_height(true, HeightmapType::WorldSurfaceWg, (x + 1).min(15), z)
        - chunk
            .access
            .get_height(true, HeightmapType::WorldSurfaceWg, (x - 1).max(0), z)
}

/// `getSurfaceGradientZ(protoChunk, x, z)`.
fn get_surface_gradient_z(chunk: &mut ProtoChunk, x: i32, z: i32) -> i32 {
    chunk
        .access
        .get_height(true, HeightmapType::WorldSurfaceWg, x, (z + 1).min(15))
        - chunk
            .access
            .get_height(true, HeightmapType::WorldSurfaceWg, x, (z - 1).max(0))
}

/// `generateBands(random)`.
fn generate_bands(random: &mut dyn RandomSource) -> Vec<BlockState> {
    let terracotta = block(Block::TERRACOTTA);
    let mut clay_bands = vec![terracotta; 192];
    let mut i = 0usize;
    while i < clay_bands.len() {
        i += random.next_int_bounded(5) as usize + 1;
        if i < clay_bands.len() {
            clay_bands[i] = block(Block::ORANGE_TERRACOTTA);
        }
        i += 1;
    }

    make_bands(random, &mut clay_bands, 1, block(Block::YELLOW_TERRACOTTA));
    make_bands(random, &mut clay_bands, 2, block(Block::BROWN_TERRACOTTA));
    make_bands(random, &mut clay_bands, 1, block(Block::RED_TERRACOTTA));
    let white_band_count = random.next_int_between_inclusive(9, 15);
    let mut count = 0;
    let mut start = 0usize;
    while count < white_band_count && start < clay_bands.len() {
        clay_bands[start] = block(Block::WHITE_TERRACOTTA);
        if start as i64 - 1 > 0 && random.next_boolean() {
            clay_bands[start - 1] = block(Block::LIGHT_GRAY_TERRACOTTA);
        }
        if start + 1 < clay_bands.len() && random.next_boolean() {
            clay_bands[start + 1] = block(Block::LIGHT_GRAY_TERRACOTTA);
        }
        count += 1;
        start += random.next_int_bounded(16) as usize + 4;
    }
    clay_bands
}

/// `makeBands(random, clayBands, baseWidth, state)`.
fn make_bands(
    random: &mut dyn RandomSource,
    clay_bands: &mut [BlockState],
    base_width: i32,
    state: BlockState,
) {
    let band_count = random.next_int_between_inclusive(6, 15);
    for _ in 0..band_count {
        let width = (base_width + random.next_int_bounded(3)) as usize;
        let start = random.next_int_bounded(clay_bands.len() as i32) as usize;
        let mut p = 0;
        while start + p < clay_bands.len() && p < width {
            clay_bands[start + p] = state;
            p += 1;
        }
    }
}
