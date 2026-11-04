//! `net.minecraft.world.level.levelgen.Aquifer`: fluid placement below the surface (noise based aquifers,
//! the disabled aquifer, `FluidPicker`/`FluidStatus`) and `Aquifer.Config.create`.
//!
//! The `SharedConstants.DEBUG_*` switches of the vanilla class are always off in a release server and not ported.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use mc_core::math::mth;
use mc_core::pos::block_pos::BlockPos;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_core::pos::quart_pos;
use mc_core::random::PositionalRandomFactory;
use mc_data::block::{Block, BlockState};

use crate::densityfunction::{DensityFunction, DensityVolume, SharedSampler};
use crate::noise_settings::{AquiferConfig, DIMENSION_MIN_Y};
use crate::random_state::DensitySamplerSet;

/// `DimensionType.WAY_BELOW_MIN_Y`.
pub const WAY_BELOW_MIN_Y: i32 = DIMENSION_MIN_Y << 4;

/// `Aquifer.FluidStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidStatus {
    pub fluid_level: i32,
    pub fluid_type: BlockState,
}

impl FluidStatus {
    pub fn new(fluid_level: i32, fluid_type: BlockState) -> FluidStatus {
        FluidStatus { fluid_level, fluid_type }
    }

    /// `at(blockY)`.
    pub fn at(&self, block_y: i32) -> BlockState {
        if block_y < self.fluid_level { self.fluid_type } else { Block::AIR.default_state() }
    }
}

/// `Aquifer.FluidPicker`: `computeFluid(blockX, blockY, blockZ)`.
pub type FluidPicker = Arc<dyn Fn(i32, i32, i32) -> FluidStatus + Send + Sync>;

/// `Aquifer`. Implementations are stateful (`shouldScheduleFluidUpdate` describes the last `compute_substance`).
pub trait Aquifer {
    /// `computeSubstance(blockX, blockY, blockZ, density)`: `None` = keep the default block (stone).
    fn compute_substance(&mut self, block_x: i32, block_y: i32, block_z: i32, density: f64) -> Option<BlockState>;

    fn should_schedule_fluid_update(&self) -> bool;
}

/// `Aquifer.createDisabled(fluidRule)`.
pub fn create_disabled(fluid_rule: FluidPicker) -> Box<dyn Aquifer> {
    Box::new(DisabledAquifer { fluid_rule })
}

struct DisabledAquifer {
    fluid_rule: FluidPicker,
}

impl Aquifer for DisabledAquifer {
    fn compute_substance(&mut self, block_x: i32, block_y: i32, block_z: i32, density: f64) -> Option<BlockState> {
        if density > 0.0 { None } else { Some((self.fluid_rule)(block_x, block_y, block_z).at(block_y)) }
    }

    fn should_schedule_fluid_update(&self) -> bool {
        false
    }
}

impl AquiferConfig {
    /// `Aquifer.Config.create(cachingSamplers, positionalRandomFactory, volume, fluidRule)`.
    pub fn create(
        &self,
        caching_samplers: Rc<DensitySamplerSet>,
        positional_random_factory: Arc<dyn PositionalRandomFactory>,
        volume: &DensityVolume,
        fluid_rule: FluidPicker,
    ) -> NoiseBasedAquifer {
        NoiseBasedAquifer::new(caching_samplers, self, positional_random_factory, volume, fluid_rule)
    }
}

/// A density function compiled for the caching sampler set (vanilla `DensitySampler.Bound`).
struct Bound {
    sampler: SharedSampler,
    samplers: Rc<DensitySamplerSet>,
}

impl Bound {
    fn new(samplers: &Rc<DensitySamplerSet>, function: &DensityFunction) -> Bound {
        Bound { sampler: samplers.random_state().get_sampler(function), samplers: samplers.clone() }
    }

    fn sample_value(&self, x: i32, y: i32, z: i32) -> f32 {
        self.sampler.sample_value(self.samplers.context(), x, y, z)
    }
}

const X_RANGE: i32 = 10;
const Y_RANGE: i32 = 9;
const Z_RANGE: i32 = 10;
/// `FLOWING_UPDATE_SIMULARITY = similarity(Mth.square(10), Mth.square(12))`.
fn flowing_update_similarity() -> f64 {
    similarity(mth::square_i32(10), mth::square_i32(12))
}

/// `SURFACE_SAMPLING_OFFSETS_IN_CHUNKS`.
const SURFACE_SAMPLING_OFFSETS_IN_CHUNKS: [[i32; 2]; 13] = [
    [0, 0],
    [-2, -1],
    [-1, -1],
    [0, -1],
    [1, -1],
    [-3, 0],
    [-2, 0],
    [-1, 0],
    [1, 0],
    [-2, 1],
    [-1, 1],
    [0, 1],
    [1, 1],
];

/// `Aquifer.NoiseBasedAquifer`.
pub struct NoiseBasedAquifer {
    barrier_noise: Bound,
    fluid_level_floodedness_noise: Bound,
    fluid_level_spread_noise: Bound,
    lava_noise: Bound,
    positional_random_factory: Arc<dyn PositionalRandomFactory>,
    aquifer_cache: Vec<Option<FluidStatus>>,
    aquifer_location_cache: Vec<i64>,
    global_fluid_picker: FluidPicker,
    exclusion: Bound,
    should_schedule_fluid_update: bool,
    skip_sampling_above_y: i32,
    min_grid_x: i32,
    min_grid_y: i32,
    min_grid_z: i32,
    grid_size_x: i32,
    grid_size_z: i32,
    surface_level: Bound,
    surface_level_cache: HashMap<i64, i32>,
    lava: BlockState,
}

impl NoiseBasedAquifer {
    fn new(
        caching_samplers: Rc<DensitySamplerSet>,
        config: &AquiferConfig,
        positional_random_factory: Arc<dyn PositionalRandomFactory>,
        volume: &DensityVolume,
        global_fluid_picker: FluidPicker,
    ) -> NoiseBasedAquifer {
        let min_grid_x = grid_x(volume.min_block_x - 5);
        let max_grid_x = grid_x(volume.max_block_x() - 5) + 1;
        let grid_size_x = max_grid_x - min_grid_x + 1;
        let min_grid_y = grid_y(volume.min_block_y + 1) - 1;
        let max_grid_y = grid_y(volume.max_block_y() + 1) + 1;
        let grid_size_y = max_grid_y - min_grid_y + 1;
        let min_grid_z = grid_z(volume.min_block_z - 5);
        let max_grid_z = grid_z(volume.max_block_z() - 5) + 1;
        let grid_size_z = max_grid_z - min_grid_z + 1;
        let total_grid_size = (grid_size_x * grid_size_y * grid_size_z) as usize;
        let mut aquifer = NoiseBasedAquifer {
            barrier_noise: Bound::new(&caching_samplers, &config.barrier),
            fluid_level_floodedness_noise: Bound::new(&caching_samplers, &config.fluid_level_floodedness),
            fluid_level_spread_noise: Bound::new(&caching_samplers, &config.fluid_level_spread),
            lava_noise: Bound::new(&caching_samplers, &config.lava),
            exclusion: Bound::new(&caching_samplers, &config.exclusion),
            surface_level: Bound::new(&caching_samplers, &config.surface_level),
            positional_random_factory,
            aquifer_cache: vec![None; total_grid_size],
            aquifer_location_cache: vec![i64::MAX; total_grid_size],
            global_fluid_picker,
            should_schedule_fluid_update: false,
            skip_sampling_above_y: 0,
            min_grid_x,
            min_grid_y,
            min_grid_z,
            grid_size_x,
            grid_size_z,
            surface_level_cache: HashMap::new(),
            lava: Block::LAVA.default_state(),
        };
        let max_surface_level = aquifer.max_surface_level(
            from_grid_x(min_grid_x, 0),
            from_grid_z(min_grid_z, 0),
            from_grid_x(max_grid_x, 9),
            from_grid_z(max_grid_z, 9),
        );
        let max_adjusted_surface_level = Self::adjust_surface_level(max_surface_level);
        let skip_sampling_above_grid_y = grid_y(max_adjusted_surface_level + 12) + 1;
        aquifer.skip_sampling_above_y = from_grid_y(skip_sampling_above_grid_y, 11) - 1;
        aquifer
    }

    fn surface_level(&mut self, block_x: i32, block_z: i32) -> i32 {
        let quantized_x = quart_pos::to_block(quart_pos::from_block(block_x));
        let quantized_z = quart_pos::to_block(quart_pos::from_block(block_z));
        let key = ChunkPos::pack_xz(quantized_x, quantized_z);
        if let Some(&level) = self.surface_level_cache.get(&key) {
            return level;
        }
        let level = mth::floor_f64(f64::from(self.surface_level.sample_value(quantized_x, 0, quantized_z)));
        self.surface_level_cache.insert(key, level);
        level
    }

    fn max_surface_level(&mut self, min_block_x: i32, min_block_z: i32, max_block_x: i32, max_block_z: i32) -> i32 {
        let min_quart_x = quart_pos::from_block(min_block_x);
        let max_quart_x = quart_pos::from_block(max_block_x);
        let min_quart_z = quart_pos::from_block(min_block_z);
        let max_quart_z = quart_pos::from_block(max_block_z);
        let volume = DensityVolume::new(
            max_quart_x - min_quart_x + 1,
            1,
            max_quart_z - min_quart_z + 1,
            quart_pos::to_block(min_quart_x),
            0,
            quart_pos::to_block(min_quart_z),
            4,
            1,
            4,
        );
        let context = self.surface_level.samplers.context();
        let mut buffer = context.acquire_buffer(&volume);
        self.surface_level.sampler.sample_volume(context, &mut buffer, &volume);
        let mut max_y = i32::MIN;
        for z in 0..volume.size_z {
            for x in 0..volume.size_x {
                let surface_level = mth::floor_f32(buffer.get(volume.index_unchecked(x, 0, z)));
                self.surface_level_cache
                    .insert(ChunkPos::pack_xz(volume.block_x(x), volume.block_z(z)), surface_level);
                if surface_level > max_y {
                    max_y = surface_level;
                }
            }
        }
        max_y
    }

    fn get_index(&self, grid_x: i32, grid_y: i32, grid_z: i32) -> usize {
        let x = grid_x - self.min_grid_x;
        let y = grid_y - self.min_grid_y;
        let z = grid_z - self.min_grid_z;
        ((y * self.grid_size_z + z) * self.grid_size_x + x) as usize
    }

    fn is_lava(&self, state: BlockState) -> bool {
        state.block() == Block::LAVA
    }

    fn is_water(&self, state: BlockState) -> bool {
        state.block() == Block::WATER
    }

    fn calculate_pressure(
        &self,
        block_x: i32,
        block_y: i32,
        block_z: i32,
        barrier_noise_value: &mut f64,
        status_closest1: &FluidStatus,
        status_closest2: &FluidStatus,
    ) -> f64 {
        let type1 = status_closest1.at(block_y);
        let type2 = status_closest2.at(block_y);
        if (!self.is_lava(type1) || !self.is_water(type2)) && (!self.is_water(type1) || !self.is_lava(type2)) {
            let fluid_y_diff = status_closest1.fluid_level.wrapping_sub(status_closest2.fluid_level).wrapping_abs();
            if fluid_y_diff == 0 {
                0.0
            } else {
                let average_fluid_y = 0.5 * f64::from(status_closest1.fluid_level.wrapping_add(status_closest2.fluid_level));
                let how_far_above_average_fluid_point = f64::from(block_y) + 0.5 - average_fluid_y;
                let base_value = f64::from(fluid_y_diff) / 2.0;
                let distance_from_barrier_edge_towards_middle = base_value - how_far_above_average_fluid_point.abs();
                let gradient = if how_far_above_average_fluid_point > 0.0 {
                    let center_point = 0.0 + distance_from_barrier_edge_towards_middle;
                    if center_point > 0.0 { center_point / 1.5 } else { center_point / 2.5 }
                } else {
                    let center_point = 3.0 + distance_from_barrier_edge_towards_middle;
                    if center_point > 0.0 { center_point / 3.0 } else { center_point / 10.0 }
                };
                let noise_value = if !(gradient < -2.0) && !(gradient > 2.0) {
                    let current_noise_value = *barrier_noise_value;
                    if current_noise_value.is_nan() {
                        let barrier_noise = f64::from(self.barrier_noise.sample_value(block_x, block_y, block_z));
                        *barrier_noise_value = barrier_noise;
                        barrier_noise
                    } else {
                        current_noise_value
                    }
                } else {
                    0.0
                };
                2.0 * (noise_value + gradient)
            }
        } else {
            2.0
        }
    }

    fn get_aquifer_status(&mut self, index: usize) -> FluidStatus {
        if let Some(old_status) = self.aquifer_cache[index] {
            return old_status;
        }
        let location = self.aquifer_location_cache[index];
        let status = self.compute_fluid(BlockPos::get_x(location), BlockPos::get_y(location), BlockPos::get_z(location));
        self.aquifer_cache[index] = Some(status);
        status
    }

    fn compute_fluid(&mut self, x: i32, y: i32, z: i32) -> FluidStatus {
        let global_fluid = (self.global_fluid_picker)(x, y, z);
        let mut lowest_preliminary_surface = i32::MAX;
        let top_of_aquifer_cell = y + 12;
        let bottom_of_aquifer_cell = y - 12;
        let mut surface_at_center_is_under_global_fluid_level = false;

        for offset in SURFACE_SAMPLING_OFFSETS_IN_CHUNKS {
            let sample_x = x + (offset[0] << 4);
            let sample_z = z + (offset[1] << 4);
            let surface_level = self.surface_level(sample_x, sample_z);
            let adjusted_surface_level = Self::adjust_surface_level(surface_level);
            let start = offset[0] == 0 && offset[1] == 0;
            if start && bottom_of_aquifer_cell > adjusted_surface_level {
                return global_fluid;
            }

            let top_of_aquifer_cell_pokes_above_surface = top_of_aquifer_cell > adjusted_surface_level;
            if top_of_aquifer_cell_pokes_above_surface || start {
                let global_fluid_at_surface = (self.global_fluid_picker)(sample_x, adjusted_surface_level, sample_z);
                if !global_fluid_at_surface.at(adjusted_surface_level).is_air() {
                    if start {
                        surface_at_center_is_under_global_fluid_level = true;
                    }
                    if top_of_aquifer_cell_pokes_above_surface {
                        return global_fluid_at_surface;
                    }
                }
            }

            lowest_preliminary_surface = lowest_preliminary_surface.min(surface_level);
        }

        let fluid_surface_level = self.compute_surface_level(
            x,
            y,
            z,
            &global_fluid,
            lowest_preliminary_surface,
            surface_at_center_is_under_global_fluid_level,
        );
        FluidStatus::new(fluid_surface_level, self.compute_fluid_type(x, y, z, &global_fluid, fluid_surface_level))
    }

    fn adjust_surface_level(preliminary_surface_level: i32) -> i32 {
        preliminary_surface_level + 8
    }

    fn compute_surface_level(
        &self,
        x: i32,
        y: i32,
        z: i32,
        global_fluid: &FluidStatus,
        lowest_surface_level: i32,
        surface_at_center_is_under_global_fluid_level: bool,
    ) -> i32 {
        let (partially_floodedness, fully_floodidness);
        if f64::from(self.exclusion.sample_value(x, y, z)) > 0.0 {
            partially_floodedness = -1.0;
            fully_floodidness = -1.0;
        } else {
            let distance_below_surface = Self::adjust_surface_level(lowest_surface_level) - y;
            let flooded_factor = if surface_at_center_is_under_global_fluid_level {
                mth::clamped_map_f64(f64::from(distance_below_surface), 0.0, 64.0, 1.0, 0.0)
            } else {
                0.0
            };
            let floodedness_noise_value =
                mth::clamp_f64(f64::from(self.fluid_level_floodedness_noise.sample_value(x, y, z)), -1.0, 1.0);
            let fully_flooded_threshold = mth::map_f64(flooded_factor, 1.0, 0.0, -0.3, 0.8);
            let partially_flooded_threshold = mth::map_f64(flooded_factor, 1.0, 0.0, -0.8, 0.4);
            partially_floodedness = floodedness_noise_value - partially_flooded_threshold;
            fully_floodidness = floodedness_noise_value - fully_flooded_threshold;
        }

        if fully_floodidness > 0.0 {
            global_fluid.fluid_level
        } else if partially_floodedness > 0.0 {
            self.compute_randomized_fluid_surface_level(x, y, z, lowest_surface_level)
        } else {
            WAY_BELOW_MIN_Y
        }
    }

    fn compute_randomized_fluid_surface_level(&self, x: i32, y: i32, z: i32, lowest_surface_level: i32) -> i32 {
        let fluid_level_cell_x = mth::floor_div(x, 16);
        let fluid_level_cell_y = mth::floor_div(y, 40);
        let fluid_level_cell_z = mth::floor_div(z, 16);
        let fluid_cell_middle_y = fluid_level_cell_y * 40 + 20;
        // `float * 10.0F` is a float multiplication widened to double afterwards.
        let fluid_level_spread = f64::from(
            self.fluid_level_spread_noise.sample_value(fluid_level_cell_x, fluid_level_cell_y, fluid_level_cell_z) * 10.0f32,
        );
        let fluid_level_spread_quantized = mth::quantize(fluid_level_spread, 3);
        let target_fluid_surface_level = fluid_cell_middle_y + fluid_level_spread_quantized;
        lowest_surface_level.min(target_fluid_surface_level)
    }

    fn compute_fluid_type(&self, x: i32, y: i32, z: i32, global_fluid: &FluidStatus, fluid_surface_level: i32) -> BlockState {
        let mut fluid_type = global_fluid.fluid_type;
        if fluid_surface_level <= -10 && fluid_surface_level != WAY_BELOW_MIN_Y && global_fluid.fluid_type != self.lava {
            let fluid_type_cell_x = mth::floor_div(x, 64);
            let fluid_type_cell_y = mth::floor_div(y, 40);
            let fluid_type_cell_z = mth::floor_div(z, 64);
            let lava_noise_value =
                f64::from(self.lava_noise.sample_value(fluid_type_cell_x, fluid_type_cell_y, fluid_type_cell_z));
            if lava_noise_value.abs() > 0.3 {
                fluid_type = self.lava;
            }
        }
        fluid_type
    }
}

impl Aquifer for NoiseBasedAquifer {
    fn compute_substance(&mut self, block_x: i32, block_y: i32, block_z: i32, density: f64) -> Option<BlockState> {
        if density > 0.0 {
            self.should_schedule_fluid_update = false;
            return None;
        }
        let global_fluid = (self.global_fluid_picker)(block_x, block_y, block_z);
        if block_y > self.skip_sampling_above_y {
            self.should_schedule_fluid_update = false;
            return Some(global_fluid.at(block_y));
        }
        if self.is_lava(global_fluid.at(block_y)) {
            self.should_schedule_fluid_update = false;
            return Some(self.lava);
        }

        let x_anchor = grid_x(block_x - 5);
        let y_anchor = grid_y(block_y + 1);
        let z_anchor = grid_z(block_z - 5);
        let mut distance_sqr1 = i32::MAX;
        let mut distance_sqr2 = i32::MAX;
        let mut distance_sqr3 = i32::MAX;
        let mut distance_sqr4 = i32::MAX;
        let mut closest_index1 = 0usize;
        let mut closest_index2 = 0usize;
        let mut closest_index3 = 0usize;
        let mut closest_index4 = 0usize;

        for x1 in 0..=1 {
            for y1 in -1..=1 {
                for z1 in 0..=1 {
                    let spaced_grid_x = x_anchor + x1;
                    let spaced_grid_y = y_anchor + y1;
                    let spaced_grid_z = z_anchor + z1;
                    let index = self.get_index(spaced_grid_x, spaced_grid_y, spaced_grid_z);
                    let existing_location = self.aquifer_location_cache[index];
                    let location = if existing_location != i64::MAX {
                        existing_location
                    } else {
                        let mut random = self.positional_random_factory.at(spaced_grid_x, spaced_grid_y, spaced_grid_z);
                        let location = BlockPos::as_long_xyz(
                            from_grid_x(spaced_grid_x, random.next_int_bounded(X_RANGE)),
                            from_grid_y(spaced_grid_y, random.next_int_bounded(Y_RANGE)),
                            from_grid_z(spaced_grid_z, random.next_int_bounded(Z_RANGE)),
                        );
                        self.aquifer_location_cache[index] = location;
                        location
                    };

                    let dx = BlockPos::get_x(location) - block_x;
                    let dy = BlockPos::get_y(location) - block_y;
                    let dz = BlockPos::get_z(location) - block_z;
                    let new_distance = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz));
                    if distance_sqr1 >= new_distance {
                        closest_index4 = closest_index3;
                        closest_index3 = closest_index2;
                        closest_index2 = closest_index1;
                        closest_index1 = index;
                        distance_sqr4 = distance_sqr3;
                        distance_sqr3 = distance_sqr2;
                        distance_sqr2 = distance_sqr1;
                        distance_sqr1 = new_distance;
                    } else if distance_sqr2 >= new_distance {
                        closest_index4 = closest_index3;
                        closest_index3 = closest_index2;
                        closest_index2 = index;
                        distance_sqr4 = distance_sqr3;
                        distance_sqr3 = distance_sqr2;
                        distance_sqr2 = new_distance;
                    } else if distance_sqr3 >= new_distance {
                        closest_index4 = closest_index3;
                        closest_index3 = index;
                        distance_sqr4 = distance_sqr3;
                        distance_sqr3 = new_distance;
                    } else if distance_sqr4 >= new_distance {
                        closest_index4 = index;
                        distance_sqr4 = new_distance;
                    }
                }
            }
        }

        let closest_status1 = self.get_aquifer_status(closest_index1);
        let similarity12 = similarity(distance_sqr1, distance_sqr2);
        let fluid_state = closest_status1.at(block_y);
        let actual_fluid_state = fluid_state;
        if similarity12 <= 0.0 {
            if similarity12 >= flowing_update_similarity() {
                let closest_status2 = self.get_aquifer_status(closest_index2);
                self.should_schedule_fluid_update = closest_status1 != closest_status2;
            } else {
                self.should_schedule_fluid_update = false;
            }
            return Some(actual_fluid_state);
        }
        if self.is_water(fluid_state)
            && self.is_lava((self.global_fluid_picker)(block_x, block_y - 1, block_z).at(block_y - 1))
        {
            self.should_schedule_fluid_update = true;
            return Some(actual_fluid_state);
        }

        let mut barrier_noise_value = f64::NAN;
        let closest_status2 = self.get_aquifer_status(closest_index2);
        let barrier12 = similarity12
            * self.calculate_pressure(block_x, block_y, block_z, &mut barrier_noise_value, &closest_status1, &closest_status2);
        if density + barrier12 > 0.0 {
            self.should_schedule_fluid_update = false;
            return None;
        }

        let closest_status3 = self.get_aquifer_status(closest_index3);
        let similarity13 = similarity(distance_sqr1, distance_sqr3);
        if similarity13 > 0.0 {
            let barrier13 = similarity12
                * similarity13
                * self.calculate_pressure(block_x, block_y, block_z, &mut barrier_noise_value, &closest_status1, &closest_status3);
            if density + barrier13 > 0.0 {
                self.should_schedule_fluid_update = false;
                return None;
            }
        }

        let similarity23 = similarity(distance_sqr2, distance_sqr3);
        if similarity23 > 0.0 {
            let barrier23 = similarity12
                * similarity23
                * self.calculate_pressure(block_x, block_y, block_z, &mut barrier_noise_value, &closest_status2, &closest_status3);
            if density + barrier23 > 0.0 {
                self.should_schedule_fluid_update = false;
                return None;
            }
        }

        let flowing = flowing_update_similarity();
        let may_flow12 = closest_status1 != closest_status2;
        let may_flow23 = similarity23 >= flowing && closest_status2 != closest_status3;
        let may_flow13 = similarity13 >= flowing && closest_status1 != closest_status3;
        if !may_flow12 && !may_flow23 && !may_flow13 {
            self.should_schedule_fluid_update = similarity13 >= flowing
                && similarity(distance_sqr1, distance_sqr4) >= flowing
                && closest_status1 != self.get_aquifer_status(closest_index4);
        } else {
            self.should_schedule_fluid_update = true;
        }
        Some(actual_fluid_state)
    }

    fn should_schedule_fluid_update(&self) -> bool {
        self.should_schedule_fluid_update
    }
}

/// `similarity(distanceSqr1, distanceSqr2)`: `1.0 - (distanceSqr2 - distanceSqr1) / 25.0`.
fn similarity(distance_sqr1: i32, distance_sqr2: i32) -> f64 {
    1.0 - f64::from(distance_sqr2.wrapping_sub(distance_sqr1)) / 25.0
}

fn grid_x(block_coord: i32) -> i32 {
    block_coord >> 4
}

fn from_grid_x(grid_coord: i32, block_offset: i32) -> i32 {
    (grid_coord << 4) + block_offset
}

fn grid_y(block_coord: i32) -> i32 {
    mth::floor_div(block_coord, 12)
}

fn from_grid_y(grid_coord: i32, block_offset: i32) -> i32 {
    grid_coord * 12 + block_offset
}

fn grid_z(block_coord: i32) -> i32 {
    block_coord >> 4
}

fn from_grid_z(grid_coord: i32, block_offset: i32) -> i32 {
    (grid_coord << 4) + block_offset
}
