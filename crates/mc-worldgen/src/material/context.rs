//! `MaterialRuleContext`: the mutable per-column / per-block state conditions and rules read, with the
//! lazily updated XZ / Y caches (`lastUpdateXZ` / `lastUpdateY` counters).

use std::collections::HashMap;
use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::PositionalRandomFactory;
use mc_data::block::BlockState;
use mc_world::palette::BiomeId;

use super::system::MaterialSystem;
use crate::biome::{Biome, BiomeRegistry};
use crate::densityfunction::{DensityBuffer, DensityFunction, DensityVolume};
use crate::random_state::{BoundSampler, DensitySamplerSet, RandomState};
use crate::synth::Noise;
use crate::vertical_anchor::VerticalAnchor;
use crate::world_generation_context::WorldGenerationContext;

/// `HOW_FAR_BELOW_PRELIMINARY_SURFACE_LEVEL_TO_BUILD_SURFACE`.
const HOW_FAR_BELOW_PRELIMINARY_SURFACE_LEVEL_TO_BUILD_SURFACE: i32 = 8;

/// A `DoubleSupplier` from `getNoiseSampler`: the noise value at the current XZ (2d) or XYZ (3d), cached
/// until the matching update counter changes.
struct NoiseSampler {
    noise: Arc<dyn Noise>,
    is_3d: bool,
    last_update: i64,
    last_noise: f64,
}

/// `MaterialRules.DensityGetter` from `getDensitiesInChunk`: the density at the context's current block.
pub(crate) struct DensityGetter<'a> {
    sampler: BoundSampler<'a>,
    /// The prefilled volume buffer (`prefill == true`).
    buffer: Option<DensityBuffer>,
}

impl DensityGetter<'_> {
    /// `get()`.
    pub(crate) fn get(&self, ctx: &MaterialRuleContext) -> f32 {
        let (x, y, z) = (ctx.block_x, ctx.block_y, ctx.block_z);
        match &self.buffer {
            Some(buffer) => match ctx.expected_volume.index_of_block(x, y, z) {
                Some(index) => buffer.get(index),
                None => self.sampler.sample_value(x, y, z),
            },
            None => self.sampler.sample_value(x, y, z),
        }
    }
}

pub struct MaterialRuleContext<'a> {
    system: &'a MaterialSystem,
    random_state: &'a RandomState,
    expected_volume: DensityVolume,
    density_samplers: &'a DensitySamplerSet,
    biome_getter: &'a dyn Fn(BlockPos) -> BiomeId,
    biomes: &'a BiomeRegistry,
    context: &'a WorldGenerationContext,
    possible_biomes: Option<&'a [BiomeId]>,
    preliminary_surface_volume: DensityVolume,
    preliminary_surface_buffer: Option<DensityBuffer>,
    noise_samplers: Vec<NoiseSampler>,
    noise_samplers_2d: HashMap<Identifier, usize>,
    noise_samplers_3d: HashMap<Identifier, usize>,
    last_update_xz: i64,
    block_x: i32,
    block_z: i32,
    surface_gradient_x: i32,
    surface_gradient_z: i32,
    surface_depth: i32,
    last_surface_depth2_update: i64,
    surface_secondary: f64,
    last_min_surface_level_update: i64,
    min_surface_level: i32,
    last_update_y: i64,
    biome: Option<BiomeId>,
    block_y: i32,
    water_height: i32,
    stone_depth_below: i32,
    stone_depth_above: i32,
}

impl<'a> MaterialRuleContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        system: &'a MaterialSystem,
        random_state: &'a RandomState,
        expected_volume: DensityVolume,
        density_samplers: &'a DensitySamplerSet,
        biome_getter: &'a dyn Fn(BlockPos) -> BiomeId,
        biomes: &'a BiomeRegistry,
        context: &'a WorldGenerationContext,
        possible_biomes: Option<&'a [BiomeId]>,
    ) -> MaterialRuleContext<'a> {
        let last_update_xz = -9223372036854775807i64;
        MaterialRuleContext {
            system,
            random_state,
            preliminary_surface_volume: DensityVolume::with_unit_step(
                expected_volume.size_x,
                1,
                expected_volume.size_z,
                expected_volume.min_block_x,
                0,
                expected_volume.min_block_z,
            ),
            expected_volume,
            density_samplers,
            biome_getter,
            biomes,
            context,
            possible_biomes,
            preliminary_surface_buffer: None,
            noise_samplers: Vec::new(),
            noise_samplers_2d: HashMap::new(),
            noise_samplers_3d: HashMap::new(),
            last_update_xz,
            block_x: 0,
            block_z: 0,
            surface_gradient_x: 0,
            surface_gradient_z: 0,
            surface_depth: 0,
            last_surface_depth2_update: last_update_xz - 1,
            surface_secondary: 0.0,
            last_min_surface_level_update: last_update_xz - 1,
            min_surface_level: 0,
            last_update_y: -9223372036854775807i64,
            biome: None,
            block_y: 0,
            water_height: 0,
            stone_depth_below: 0,
            stone_depth_above: 0,
        }
    }

    /// `updateXZ(blockX, blockZ, surfaceGradientX, surfaceGradientZ)`.
    pub(crate) fn update_xz(
        &mut self,
        block_x: i32,
        block_z: i32,
        surface_gradient_x: i32,
        surface_gradient_z: i32,
    ) {
        self.last_update_xz = self.last_update_xz.wrapping_add(1);
        self.last_update_y = self.last_update_y.wrapping_add(1);
        self.block_x = block_x;
        self.block_z = block_z;
        self.surface_gradient_x = surface_gradient_x;
        self.surface_gradient_z = surface_gradient_z;
        self.surface_depth = self.system.get_surface_depth(block_x, block_z);
    }

    /// `updateY(stoneDepthAbove, stoneDepthBelow, waterHeight, blockY)`.
    pub(crate) fn update_y(
        &mut self,
        stone_depth_above: i32,
        stone_depth_below: i32,
        water_height: i32,
        block_y: i32,
    ) {
        self.last_update_y = self.last_update_y.wrapping_add(1);
        self.biome = None;
        self.block_y = block_y;
        self.water_height = water_height;
        self.stone_depth_below = stone_depth_below;
        self.stone_depth_above = stone_depth_above;
    }

    pub(crate) fn last_update_xz(&self) -> i64 {
        self.last_update_xz
    }

    pub(crate) fn last_update_y(&self) -> i64 {
        self.last_update_y
    }

    /// `getSurfaceSecondary()`.
    pub(crate) fn get_surface_secondary(&mut self) -> f64 {
        if self.last_surface_depth2_update != self.last_update_xz {
            self.last_surface_depth2_update = self.last_update_xz;
            self.surface_secondary = self.system.get_surface_secondary(self.block_x, self.block_z);
        }
        self.surface_secondary
    }

    /// `getBiome()`.
    pub(crate) fn get_biome(&mut self) -> BiomeId {
        match self.biome {
            Some(biome) => biome,
            None => {
                let biome = (self.biome_getter)(BlockPos::new(self.block_x, self.block_y, self.block_z));
                self.biome = Some(biome);
                biome
            }
        }
    }

    /// The registry data of a biome handed out by [`Self::get_biome`].
    pub(crate) fn biome_data(&self, biome: BiomeId) -> &'a Biome {
        self.biomes.get(biome)
    }

    /// `getSeaLevel()`.
    pub(crate) fn sea_level(&self) -> i32 {
        self.system.get_sea_level()
    }

    /// `getMinSurfaceLevel()`.
    pub(crate) fn get_min_surface_level(&mut self) -> i32 {
        if self.last_min_surface_level_update != self.last_update_xz {
            self.last_min_surface_level_update = self.last_update_xz;
            let function = self.system.preliminary_surface_function();
            let preliminary_surface_level = match self
                .preliminary_surface_volume
                .index_of_block(self.block_x, 0, self.block_z)
            {
                Some(index) => {
                    if self.preliminary_surface_buffer.is_none() {
                        let mut buffer =
                            DensityBuffer::create_unpooled(self.preliminary_surface_volume.size());
                        self.density_samplers
                            .get(function)
                            .sample_volume(&mut buffer, &self.preliminary_surface_volume);
                        self.preliminary_surface_buffer = Some(buffer);
                    }
                    self.preliminary_surface_buffer.as_ref().map_or(0.0, |b| b.get(index))
                }
                None => self.density_samplers.sample_value(function, self.block_x, 0, self.block_z),
            };
            self.min_surface_level = mth::floor_f32(preliminary_surface_level)
                .wrapping_add(self.surface_depth)
                .wrapping_sub(HOW_FAR_BELOW_PRELIMINARY_SURFACE_LEVEL_TO_BUILD_SURFACE);
        }
        self.min_surface_level
    }

    /// `getNoiseSampler(noiseId, is3d)`: the index of the (shared) sampler, created on first use.
    pub(crate) fn get_noise_sampler(&mut self, noise_id: &Identifier, is_3d: bool) -> usize {
        let existing = if is_3d { &self.noise_samplers_3d } else { &self.noise_samplers_2d };
        if let Some(&index) = existing.get(noise_id) {
            return index;
        }
        let noise = self
            .random_state
            .get_or_create_noise(noise_id)
            .expect("noise referenced by a material condition is registered");
        let last_update = if is_3d { self.last_update_y } else { self.last_update_xz }.wrapping_sub(1);
        let index = self.noise_samplers.len();
        self.noise_samplers.push(NoiseSampler { noise, is_3d, last_update, last_noise: 0.0 });
        if is_3d {
            self.noise_samplers_3d.insert(noise_id.clone(), index);
        } else {
            self.noise_samplers_2d.insert(noise_id.clone(), index);
        }
        index
    }

    /// `DoubleSupplier.getAsDouble()` of a sampler from [`Self::get_noise_sampler`].
    pub(crate) fn noise_sampler_value(&mut self, index: usize) -> f64 {
        let counter = if self.noise_samplers[index].is_3d { self.last_update_y } else { self.last_update_xz };
        let (x, y, z) = (f64::from(self.block_x), f64::from(self.block_y), f64::from(self.block_z));
        let sampler = &mut self.noise_samplers[index];
        if sampler.last_update != counter {
            sampler.last_noise = if sampler.is_3d {
                f64::from(sampler.noise.get3(x, y, z))
            } else {
                f64::from(sampler.noise.get3(x, 0.0, z))
            };
            sampler.last_update = counter;
        }
        sampler.last_noise
    }

    /// `getOrCreateRandomFactory(name)`.
    pub(crate) fn get_or_create_random_factory(
        &self,
        name: &Identifier,
    ) -> Arc<dyn PositionalRandomFactory> {
        self.random_state.get_or_create_random_factory(name)
    }

    /// `getDensitiesInChunk(function, prefill)`.
    pub(crate) fn get_densities_in_chunk(
        &self,
        function: &DensityFunction,
        prefill: bool,
    ) -> DensityGetter<'a> {
        let sampler = self.density_samplers.get(function);
        let buffer = prefill.then(|| {
            let mut buffer = DensityBuffer::create_unpooled(self.expected_volume.size());
            sampler.sample_volume(&mut buffer, &self.expected_volume);
            buffer
        });
        DensityGetter { sampler, buffer }
    }

    /// `possibleBiomes()`.
    pub(crate) fn possible_biomes(&self) -> Option<&'a [BiomeId]> {
        self.possible_biomes
    }

    pub(crate) fn stone_depth_above(&self) -> i32 {
        self.stone_depth_above
    }

    pub(crate) fn stone_depth_below(&self) -> i32 {
        self.stone_depth_below
    }

    pub(crate) fn surface_depth(&self) -> i32 {
        self.surface_depth
    }

    pub(crate) fn water_height(&self) -> i32 {
        self.water_height
    }

    pub(crate) fn block_x(&self) -> i32 {
        self.block_x
    }

    pub(crate) fn block_y(&self) -> i32 {
        self.block_y
    }

    pub(crate) fn block_z(&self) -> i32 {
        self.block_z
    }

    /// `blockPos()`.
    pub(crate) fn block_pos(&self) -> BlockPos {
        BlockPos::new(self.block_x, self.block_y, self.block_z)
    }

    pub(crate) fn surface_gradient_x(&self) -> i32 {
        self.surface_gradient_x
    }

    pub(crate) fn surface_gradient_z(&self) -> i32 {
        self.surface_gradient_z
    }

    /// `resolveAnchorY(anchor)`.
    pub(crate) fn resolve_anchor_y(&self, anchor: &VerticalAnchor) -> i32 {
        anchor.resolve_y(self.context)
    }

    /// `getBand(x, y, z)`.
    pub(crate) fn get_band(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.system.get_band(x, y, z)
    }
}
