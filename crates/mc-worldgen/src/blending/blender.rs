//! `levelgen/blending/Blender.java`: blends new terrain into old (pre-1.18) chunks around it.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use mc_core::direction::{Direction, Direction8, Directional};
use mc_core::math::mth;
use mc_core::pos::{BlockPos, ChunkPos, quart_pos};
use mc_core::random::XoroshiroRandomSource;
use mc_world::blending_data::{BlendingData, NO_VALUE};
use mc_world::chunk::LevelHeight;
use mc_world::heightmap::HeightmapType;
use mc_world::palette::BiomeId;

use super::data::{BlendingDataExt, get_or_update_blending_data, is_leaves};
use super::fastutil;
use super::region::{BlendingChunk, BlendingRegion};
use crate::densityfunction::sampler::{
    self, DensityBuffer, DensitySampler, DensityVolume, SamplerContext, SharedSampler,
};
use crate::synth::{Noise, NormalNoise};

/// `HEIGHT_BLENDING_RANGE_CELLS`.
const HEIGHT_BLENDING_RANGE_CELLS: i32 = quart_pos::from_section(7) - 1;
/// `HEIGHT_BLENDING_RANGE_CHUNKS`.
const HEIGHT_BLENDING_RANGE_CHUNKS: i32 = quart_pos::to_section(HEIGHT_BLENDING_RANGE_CELLS + 3);
/// `DENSITY_BLENDING_RANGE_CHUNKS`.
const DENSITY_BLENDING_RANGE_CHUNKS: i32 = quart_pos::to_section(5);

/// `NoiseData.DEFAULT_SHIFT.create(new XoroshiroRandomSource(42L))`.
static SHIFT_NOISE: LazyLock<Arc<dyn Noise>> = LazyLock::new(|| {
    NormalNoise::create_parity(-3, &[1.0, 1.0, 1.0, 0.0]).create(&mut XoroshiroRandomSource::new(42))
});

/// `CarvingMask.Filter`, owning.
pub type CarvingFilter = Arc<dyn Fn(i32, i32, i32) -> bool + Send + Sync>;

/// `Blender.BlendingOutput`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendingOutput {
    pub alpha: f32,
    pub blending_offset: f32,
}

/// Blending data of the chunks around the region's center, in the iteration order of vanilla's
/// `Long2ObjectOpenHashMap` (see [`fastutil`]).
struct ChunkData {
    entries: Vec<(i64, Arc<BlendingData>)>,
}

impl ChunkData {
    fn new(mut map: HashMap<i64, Arc<BlendingData>>, insertion_order: &[i64]) -> ChunkData {
        let entries = fastutil::iteration_order(insertion_order)
            .into_iter()
            .map(|key| (key, map.remove(&key).expect("key was inserted")))
            .collect();
        ChunkData { entries }
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

struct Inner {
    height_and_biome: ChunkData,
    height_and_biome_by_pos: HashMap<i64, usize>,
    density: ChunkData,
    carving_filter: Option<CarvingFilter>,
}

/// A `Blender`. Cheap to clone (shared state); [`Blender::empty`] is vanilla's `Blender.EMPTY`.
#[derive(Clone, Default)]
pub struct Blender {
    /// `None` = `EMPTY` (never holds an empty `Inner`).
    inner: Option<Arc<Inner>>,
}

/// `Blender.DistanceGetter`.
pub trait DistanceGetter: Send + Sync {
    fn get_distance(&self, x: f64, y: f64, z: f64) -> f64;
}

/// `makeOffsetOldChunkDistanceGetter`.
struct OffsetOldChunkDistanceGetter {
    offset_x: f64,
    offset_z: f64,
    old_chunk_y_radius: f64,
    old_chunk_center_y: f64,
}

impl OffsetOldChunkDistanceGetter {
    fn new(offset: Option<Direction8>, area: LevelHeight) -> Self {
        let mut offset_x = 0.0;
        let mut offset_z = 0.0;
        if let Some(offset) = offset {
            for direction in offset.get_directions() {
                offset_x += (direction.get_step_x() * 16) as f64;
                offset_z += (direction.get_step_z() * 16) as f64;
            }
        }
        let old_chunk_y_radius = area.height as f64 / 2.0;
        let old_chunk_center_y = area.min_y as f64 + old_chunk_y_radius;
        OffsetOldChunkDistanceGetter { offset_x, offset_z, old_chunk_y_radius, old_chunk_center_y }
    }
}

impl DistanceGetter for OffsetOldChunkDistanceGetter {
    fn get_distance(&self, x: f64, y: f64, z: f64) -> f64 {
        distance_to_cube(
            x - 8.0 - self.offset_x,
            y - self.old_chunk_center_y,
            z - 8.0 - self.offset_z,
            8.0,
            self.old_chunk_y_radius,
            8.0,
        )
    }
}

/// `makeOldChunkDistanceGetter(centerBlendingData, oldSidesBlendingData)`.
struct ClosestDistanceGetter(Vec<OffsetOldChunkDistanceGetter>);

impl DistanceGetter for ClosestDistanceGetter {
    fn get_distance(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut closest = f64::INFINITY;
        for getter in &self.0 {
            let distance = getter.get_distance(x, y, z);
            if distance < closest {
                closest = distance;
            }
        }
        closest
    }
}

/// `Blender.makeOldChunkDistanceGetter`, over the `getAreaWithOldGeneration()` of the blending data;
/// `old_sides_area` in `Direction8` order.
pub fn make_old_chunk_distance_getter(
    center_area: Option<LevelHeight>,
    old_sides_area: &[(Direction8, LevelHeight)],
) -> impl DistanceGetter + use<> {
    let mut getters = Vec::new();
    if let Some(center) = center_area {
        getters.push(OffsetOldChunkDistanceGetter::new(None, center));
    }
    for (side, area) in old_sides_area {
        getters.push(OffsetOldChunkDistanceGetter::new(Some(*side), *area));
    }
    ClosestDistanceGetter(getters)
}

/// `distanceToCube`.
fn distance_to_cube(x: f64, y: f64, z: f64, radius_x: f64, radius_y: f64, radius_z: f64) -> f64 {
    let delta_x = x.abs() - radius_x;
    let delta_y = y.abs() - radius_y;
    let delta_z = z.abs() - radius_z;
    mth::length_xyz_f64(java_max(0.0, delta_x), java_max(0.0, delta_y), java_max(0.0, delta_z))
}

/// `Math.max(double, double)`.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

/// `heightToOffset`.
fn height_to_offset(height: f64) -> f64 {
    let dimension_factor = 1.0;
    let target_y = height + 0.5;
    let target_y_mod = mth::positive_modulo_f64(target_y, 8.0);
    dimension_factor * (32.0 * (target_y - 128.0) - 3.0 * (target_y - 120.0) * target_y_mod + 3.0 * target_y_mod * target_y_mod)
        / (128.0 * (32.0 - 3.0 * target_y_mod))
}

/// `createAroundOldChunksCarvingMaskFilter(region, chunk)`.
fn create_around_old_chunks_carving_mask_filter(region: &dyn BlendingRegion, chunk_pos: ChunkPos) -> Option<CarvingFilter> {
    let mut old_sides: Vec<(Direction8, LevelHeight)> = Vec::new();
    for direction8 in Direction8::ALL {
        let test_chunk_x = chunk_pos.x + direction8.get_step_x();
        let test_chunk_z = chunk_pos.z + direction8.get_step_z();
        region.with_chunk(test_chunk_x, test_chunk_z, &mut |chunk| {
            if let Some(data) = chunk.blending_data() {
                old_sides.push((direction8, data.get_area_with_old_generation()));
            }
        });
    }
    let mut center = None;
    region.with_chunk(chunk_pos.x, chunk_pos.z, &mut |chunk| center = chunk.blending_data().map(|data| data.get_area_with_old_generation()));
    if center.is_none() && old_sides.is_empty() {
        return None;
    }
    let distance_getter = make_old_chunk_distance_getter(center, &old_sides);
    Some(Arc::new(move |x, y, z| {
        let (xd, yd, zd) = (f64::from(x), f64::from(y), f64::from(z));
        let shift = &**SHIFT_NOISE;
        let shifted_x = xd + 0.5 + f64::from(shift.get3(xd, yd, zd) * 4.0f32);
        let shifted_y = yd + 0.5 + f64::from(shift.get3(yd, zd, xd) * 4.0f32);
        let shifted_z = zd + 0.5 + f64::from(shift.get3(zd, xd, yd) * 4.0f32);
        distance_getter.get_distance(shifted_x, shifted_y, shifted_z) < 4.0
    }))
}

impl Blender {
    /// `Blender.empty()`.
    pub fn empty() -> Blender {
        Blender { inner: None }
    }

    /// `Blender.of(region)`; `None` stands for vanilla's `null` region.
    pub fn of(region: Option<&dyn BlendingRegion>) -> Blender {
        let Some(region) = region else {
            return Blender::empty();
        };
        let center_pos = region.get_center();
        if !region.is_old_chunk_around(center_pos, HEIGHT_BLENDING_RANGE_CHUNKS) {
            return Blender::empty();
        }
        let mut height_and_biome_data = HashMap::new();
        let mut height_and_biome_order = Vec::new();
        let mut density_data = HashMap::new();
        let mut density_order = Vec::new();
        let max_dist_sq = mth::square_i32(HEIGHT_BLENDING_RANGE_CHUNKS + 1);
        for dx in -HEIGHT_BLENDING_RANGE_CHUNKS..=HEIGHT_BLENDING_RANGE_CHUNKS {
            for dz in -HEIGHT_BLENDING_RANGE_CHUNKS..=HEIGHT_BLENDING_RANGE_CHUNKS {
                if dx * dx + dz * dz <= max_dist_sq {
                    let chunk_x = center_pos.x + dx;
                    let chunk_z = center_pos.z + dz;
                    if let Some(blending_data) = get_or_update_blending_data(region, chunk_x, chunk_z) {
                        let blending_data = Arc::new(blending_data);
                        let key = ChunkPos::pack_xz(chunk_x, chunk_z);
                        height_and_biome_data.insert(key, blending_data.clone());
                        height_and_biome_order.push(key);
                        if (-DENSITY_BLENDING_RANGE_CHUNKS..=DENSITY_BLENDING_RANGE_CHUNKS).contains(&dx)
                            && (-DENSITY_BLENDING_RANGE_CHUNKS..=DENSITY_BLENDING_RANGE_CHUNKS).contains(&dz)
                        {
                            density_data.insert(key, blending_data);
                            density_order.push(key);
                        }
                    }
                }
            }
        }
        let carving_filter = create_around_old_chunks_carving_mask_filter(region, center_pos);
        let height_and_biome = ChunkData::new(height_and_biome_data, &height_and_biome_order);
        let density = ChunkData::new(density_data, &density_order);
        if height_and_biome.is_empty() && density.is_empty() && carving_filter.is_none() {
            return Blender::empty();
        }
        let height_and_biome_by_pos = height_and_biome.entries.iter().enumerate().map(|(i, (key, _))| (*key, i)).collect();
        Blender { inner: Some(Arc::new(Inner { height_and_biome, height_and_biome_by_pos, density, carving_filter })) }
    }

    /// `isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.inner.is_none()
    }

    /// `getCarvingFilter()`.
    pub fn get_carving_filter(&self) -> Option<Box<dyn Fn(i32, i32, i32) -> bool + Send + Sync>> {
        let filter = self.inner.as_ref()?.carving_filter.clone()?;
        Some(Box::new(move |x, y, z| filter(x, y, z)))
    }

    /// `blendOffsetAndFactor(volume)`.
    pub fn blend_offset_and_factor(&self, volume: &DensityVolume) -> OutputBuffer {
        let min_cell_x = quart_pos::from_block(volume.min_block_x);
        let min_cell_z = quart_pos::from_block(volume.min_block_z);
        let max_cell_x = quart_pos::from_block(volume.max_block_x()) + 1;
        let max_cell_z = quart_pos::from_block(volume.max_block_z()) + 1;
        let cell_count_x = max_cell_x - min_cell_x + 1;
        let cell_count_z = max_cell_z - min_cell_z + 1;
        let mut alphas = vec![0.0f32; (cell_count_x * cell_count_z) as usize];
        let mut offsets = vec![0.0f32; (cell_count_x * cell_count_z) as usize];
        for cell_z in 0..cell_count_z {
            for cell_x in 0..cell_count_x {
                let index = (cell_x + cell_z * cell_count_x) as usize;
                let output =
                    self.blend_offset_and_factor_at(quart_pos::to_block(min_cell_x + cell_x), quart_pos::to_block(min_cell_z + cell_z));
                alphas[index] = output.alpha;
                offsets[index] = output.blending_offset;
            }
        }
        OutputBuffer {
            blender: self.clone(),
            min_cell_x,
            min_cell_z,
            cell_count_x,
            cell_count_z,
            alphas: alphas.into(),
            offsets: offsets.into(),
        }
    }

    /// `blendOffsetAndFactor(blockX, blockZ)` (protected in vanilla).
    pub fn blend_offset_and_factor_at(&self, block_x: i32, block_z: i32) -> BlendingOutput {
        let Some(inner) = &self.inner else {
            return BlendingOutput { alpha: 1.0, blending_offset: 0.0 };
        };
        let cell_x = quart_pos::from_block(block_x);
        let cell_z = quart_pos::from_block(block_z);
        let fixed_height = inner.get_blending_data_value(cell_x, 0, cell_z, &|data, x, y, z| data.get_height(x, y, z));
        if fixed_height != NO_VALUE {
            return BlendingOutput { alpha: 0.0, blending_offset: height_to_offset(f64::from(fixed_height)) as f32 };
        }
        let mut total_weight = 0.0f64;
        let mut weighted_heights = 0.0f64;
        let mut closest_distance = f64::INFINITY;
        for (chunk_pos, blending_data) in &inner.height_and_biome.entries {
            blending_data.iterate_heights(
                quart_pos::from_section(ChunkPos::get_x(*chunk_pos)),
                quart_pos::from_section(ChunkPos::get_z(*chunk_pos)),
                &mut |test_cell_x, test_cell_z, height| {
                    let distance = f64::from(mth::length_f32((cell_x - test_cell_x) as f32, (cell_z - test_cell_z) as f32));
                    if !(distance > f64::from(HEIGHT_BLENDING_RANGE_CELLS)) {
                        if distance < closest_distance {
                            closest_distance = distance;
                        }
                        let weight = 1.0 / (distance * distance * distance * distance);
                        weighted_heights += f64::from(height) * weight;
                        total_weight += weight;
                    }
                },
            );
        }
        if closest_distance == f64::INFINITY {
            BlendingOutput { alpha: 1.0, blending_offset: 0.0 }
        } else {
            let average_height = weighted_heights / total_weight;
            let mut alpha = mth::clamp_f64(closest_distance / f64::from(HEIGHT_BLENDING_RANGE_CELLS + 1), 0.0, 1.0);
            alpha = 3.0 * alpha * alpha - 2.0 * alpha * alpha * alpha;
            BlendingOutput { alpha: alpha as f32, blending_offset: height_to_offset(average_height) as f32 }
        }
    }

    /// `blendDensity(blockX, blockY, blockZ, noiseValue)`.
    pub fn blend_density(&self, block_x: i32, block_y: i32, block_z: i32, noise_value: f32) -> f32 {
        let Some(inner) = &self.inner else {
            return noise_value;
        };
        let cell_x = quart_pos::from_block(block_x);
        let cell_y = block_y / 8;
        let cell_z = quart_pos::from_block(block_z);
        let fixed_density = inner.get_blending_data_value(cell_x, cell_y, cell_z, &|data, x, y, z| data.get_density(x, y, z));
        if fixed_density != NO_VALUE {
            return fixed_density;
        }
        let mut total_weight = 0.0f64;
        let mut weighted_heights = 0.0f64;
        let mut closest_distance = f64::INFINITY;
        for (chunk_pos, blending_data) in &inner.density.entries {
            blending_data.iterate_densities(
                quart_pos::from_section(ChunkPos::get_x(*chunk_pos)),
                quart_pos::from_section(ChunkPos::get_z(*chunk_pos)),
                cell_y - 1,
                cell_y + 1,
                &mut |test_cell_x, test_cell_y, test_cell_z, density| {
                    let distance = f64::from(mth::length_xyz_f32(
                        (cell_x - test_cell_x) as f32,
                        ((cell_y - test_cell_y) * 2) as f32,
                        (cell_z - test_cell_z) as f32,
                    ));
                    if !(distance > 2.0) {
                        if distance < closest_distance {
                            closest_distance = distance;
                        }
                        let weight = 1.0 / (distance * distance * distance * distance);
                        weighted_heights += f64::from(density) * weight;
                        total_weight += weight;
                    }
                },
            );
        }
        if closest_distance == f64::INFINITY {
            noise_value
        } else {
            let average_density = weighted_heights / total_weight;
            let alpha = mth::clamp_f64(closest_distance / 3.0, 0.0, 1.0);
            mth::lerp_f64(alpha, average_density, f64::from(noise_value)) as f32
        }
    }

    /// `getBiomeResolver(biomeResolver)`: the blended biome where old chunks dictate one, else `biome_resolver`'s.
    pub fn get_biome_resolver<'a>(
        &'a self,
        biome_resolver: &'a (dyn Fn(i32, i32, i32) -> BiomeId + 'a),
    ) -> Box<dyn Fn(i32, i32, i32) -> BiomeId + 'a> {
        if self.is_empty() {
            return Box::new(move |x, y, z| biome_resolver(x, y, z));
        }
        Box::new(move |quart_x, quart_y, quart_z| {
            self.blend_biome(quart_x, quart_y, quart_z).unwrap_or_else(|| biome_resolver(quart_x, quart_y, quart_z))
        })
    }

    /// `blendBiome(quartX, quartY, quartZ)`.
    pub fn blend_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> Option<BiomeId> {
        let inner = self.inner.as_ref()?;
        let mut closest_distance = f32::INFINITY;
        let mut closest_biome = None;
        for (chunk_pos, blending_data) in &inner.height_and_biome.entries {
            blending_data.iterate_biomes(
                quart_pos::from_section(ChunkPos::get_x(*chunk_pos)),
                quart_y,
                quart_pos::from_section(ChunkPos::get_z(*chunk_pos)),
                &mut |test_cell_x, test_cell_z, biome| {
                    let distance = mth::length_f32((quart_x - test_cell_x) as f32, (quart_z - test_cell_z) as f32);
                    if !(distance > HEIGHT_BLENDING_RANGE_CELLS as f32) && distance < closest_distance {
                        closest_biome = Some(biome);
                        closest_distance = distance;
                    }
                },
            );
        }
        if closest_distance == f32::INFINITY {
            None
        } else {
            let shift_noise = SHIFT_NOISE.get3(f64::from(quart_x), 0.0, f64::from(quart_z)) * 12.0f32;
            let alpha = mth::clamp_f32((closest_distance + shift_noise) / (HEIGHT_BLENDING_RANGE_CELLS + 1) as f32, 0.0, 1.0);
            if alpha > 0.5 { None } else { closest_biome }
        }
    }
}

impl Inner {
    /// `getBlendingDataValue(cellX, cellY, cellZ, getter)`.
    fn get_blending_data_value(&self, cell_x: i32, cell_y: i32, cell_z: i32, getter: &dyn Fn(&BlendingData, i32, i32, i32) -> f32) -> f32 {
        let chunk_x = quart_pos::to_section(cell_x);
        let chunk_z = quart_pos::to_section(cell_z);
        let min_x = (cell_x & 3) == 0;
        let min_z = (cell_z & 3) == 0;
        let mut value = self.value_in_chunk(getter, chunk_x, chunk_z, cell_x, cell_y, cell_z);
        if value == NO_VALUE {
            if min_x && min_z {
                value = self.value_in_chunk(getter, chunk_x - 1, chunk_z - 1, cell_x, cell_y, cell_z);
            }
            if value == NO_VALUE {
                if min_x {
                    value = self.value_in_chunk(getter, chunk_x - 1, chunk_z, cell_x, cell_y, cell_z);
                }
                if value == NO_VALUE && min_z {
                    value = self.value_in_chunk(getter, chunk_x, chunk_z - 1, cell_x, cell_y, cell_z);
                }
            }
        }
        value
    }

    fn value_in_chunk(
        &self,
        getter: &dyn Fn(&BlendingData, i32, i32, i32) -> f32,
        chunk_x: i32,
        chunk_z: i32,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
    ) -> f32 {
        match self.height_and_biome_by_pos.get(&ChunkPos::pack_xz(chunk_x, chunk_z)) {
            Some(&index) => getter(
                &self.height_and_biome.entries[index].1,
                cell_x - quart_pos::from_section(chunk_x),
                cell_y,
                cell_z - quart_pos::from_section(chunk_z),
            ),
            None => NO_VALUE,
        }
    }
}

impl sampler::Blender for Blender {
    fn is_empty(&self) -> bool {
        Blender::is_empty(self)
    }

    fn blend_density(&self, block_x: i32, block_y: i32, block_z: i32, input: f32) -> f32 {
        Blender::blend_density(self, block_x, block_y, block_z, input)
    }
}

/// `Blender.OutputBuffer`.
#[derive(Clone)]
pub struct OutputBuffer {
    pub blender: Blender,
    pub min_cell_x: i32,
    pub min_cell_z: i32,
    pub cell_count_x: i32,
    pub cell_count_z: i32,
    pub alphas: Arc<[f32]>,
    pub offsets: Arc<[f32]>,
}

impl OutputBuffer {
    /// `createAlphaSampler()` (for `BLENDER_ALPHA_KEY`).
    pub fn create_alpha_sampler(&self) -> SharedSampler {
        Arc::new(BufferSampler { buffer: self.clone(), offset: false })
    }

    /// `createOffsetSampler()` (for `BLENDER_OFFSET_KEY`).
    pub fn create_offset_sampler(&self) -> SharedSampler {
        Arc::new(BufferSampler { buffer: self.clone(), offset: true })
    }

    /// `getAlpha(blockX, blockZ)`.
    pub fn get_alpha(&self, block_x: i32, block_z: i32) -> f32 {
        match self.get_index(block_x, block_z) {
            Some(index) => self.alphas[index],
            None => self.blender.blend_offset_and_factor_at(block_x, block_z).alpha,
        }
    }

    /// `getOffset(blockX, blockZ)`.
    pub fn get_offset(&self, block_x: i32, block_z: i32) -> f32 {
        match self.get_index(block_x, block_z) {
            Some(index) => self.offsets[index],
            None => self.blender.blend_offset_and_factor_at(block_x, block_z).blending_offset,
        }
    }

    fn get_index(&self, block_x: i32, block_z: i32) -> Option<usize> {
        let relative_x = quart_pos::from_block(block_x) - self.min_cell_x;
        let relative_z = quart_pos::from_block(block_z) - self.min_cell_z;
        (relative_x >= 0 && relative_z >= 0 && relative_x < self.cell_count_x && relative_z < self.cell_count_z)
            .then(|| (relative_x + relative_z * self.cell_count_x) as usize)
    }
}

/// The anonymous samplers of `createAlphaSampler` / `createOffsetSampler`.
struct BufferSampler {
    buffer: OutputBuffer,
    offset: bool,
}

impl DensitySampler for BufferSampler {
    fn sample_volume(&self, context: &SamplerContext, out: &mut DensityBuffer, volume: &DensityVolume) {
        sampler::sample_volume_naive(context, out, volume, self);
    }

    fn sample_value(&self, _context: &SamplerContext, block_x: i32, _block_y: i32, block_z: i32) -> f32 {
        if self.offset { self.buffer.get_offset(block_x, block_z) } else { self.buffer.get_alpha(block_x, block_z) }
    }
}

/// `Blender.generateBorderTicks(region, chunk)` for the region's chunk at `chunk_pos` (vanilla passes the chunk object).
pub fn generate_border_ticks(region: &dyn BlendingRegion, chunk_pos: ChunkPos) {
    let mut neighbor_old = [false; 4];
    for (i, direction) in Direction::HORIZONTAL.iter().enumerate() {
        region.with_chunk(chunk_pos.x + direction.get_step_x(), chunk_pos.z + direction.get_step_z(), &mut |chunk| {
            neighbor_old[i] = chunk.is_old_noise_generation();
        });
    }
    region.with_chunk(chunk_pos.x, chunk_pos.z, &mut |chunk| {
        let old_noise_generation = chunk.is_old_noise_generation();
        let chunk_origin = BlockPos::new(chunk_pos.get_min_block_x(), 0, chunk_pos.get_min_block_z());
        let Some(area) = chunk.blending_data().map(|data| data.get_area_with_old_generation()) else {
            return;
        };
        let (old_min_y, old_max_y) = (area.min_y, area.max_y());
        if old_noise_generation {
            for x in 0..16 {
                for z in 0..16 {
                    generate_border_tick(chunk, chunk_origin.offset(x, old_min_y - 1, z));
                    generate_border_tick(chunk, chunk_origin.offset(x, old_min_y, z));
                    generate_border_tick(chunk, chunk_origin.offset(x, old_max_y, z));
                    generate_border_tick(chunk, chunk_origin.offset(x, old_max_y + 1, z));
                }
            }
        }
        for (i, direction) in Direction::HORIZONTAL.iter().enumerate() {
            if neighbor_old[i] != old_noise_generation {
                let min_x = if *direction == Direction::East { 15 } else { 0 };
                let max_x = if *direction == Direction::West { 0 } else { 15 };
                let min_z = if *direction == Direction::South { 15 } else { 0 };
                let max_z = if *direction == Direction::North { 0 } else { 15 };
                for x in min_x..=max_x {
                    for z in min_z..=max_z {
                        let max_y = old_max_y.min(chunk.get_height_primed(HeightmapType::MotionBlocking, x, z)) + 1;
                        for y in old_min_y..max_y {
                            generate_border_tick(chunk, chunk_origin.offset(x, y, z));
                        }
                    }
                }
            }
        }
    });
}

/// `generateBorderTick(chunk, pos)`.
fn generate_border_tick(chunk: &mut dyn BlendingChunk, pos: BlockPos) {
    if is_leaves(chunk.get_block_state(pos)) {
        chunk.mark_pos_for_post_processing(pos);
    }
    if !chunk.get_fluid_state(pos).is_empty() {
        chunk.mark_pos_for_post_processing(pos);
    }
}
