//! Sampling infrastructure of the compiled density function engine:
//! `DensityVolume`, `DensityBuffer`, `SamplerContext`, `DensitySampler` and friends
//! (net.minecraft.world.level.levelgen.densityfunction.{DensityVolume, DensityBuffer, SamplerContext,
//! DensitySampler, CachingDensitySampler, ContextBoundSampler, ScopedDensityBuffer}).

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::util::{floor_div, floor_mod};

/// Compiled sampler of one density function. Implementations must be immutable after construction;
/// all per-sampling state lives in the [`SamplerContext`].
pub trait DensitySampler: Send + Sync {
    /// Fills `out` (size `volume.size()`, indexing = `DensityVolume::index_unchecked`) with the values.
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    );
    fn sample_value(
        &self,
        context: &SamplerContext,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32;
}

/// `DensitySampler.sampleVolumeNaive`.
pub fn sample_volume_naive(
    context: &SamplerContext,
    out: &mut DensityBuffer,
    volume: &DensityVolume,
    sampler: &dyn DensitySampler,
) {
    let mut index = 0;
    for z in 0..volume.size_z {
        let block_z = volume.block_z(z);
        for x in 0..volume.size_x {
            let block_x = volume.block_x(x);
            for y in 0..volume.size_y {
                let block_y = volume.block_y(y);
                out.set(
                    index,
                    sampler.sample_value(context, block_x, block_y, block_z),
                );
                index += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// DensityVolume
// ---------------------------------------------------------------------------------------------

/// A regular 3D grid of block positions: `block(i) = min_block + i * step_block`.
/// Index layout: `index = y + (x + z * size_x) * size_y`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DensityVolume {
    pub size_x: i32,
    pub size_y: i32,
    pub size_z: i32,
    pub min_block_x: i32,
    pub min_block_y: i32,
    pub min_block_z: i32,
    pub step_block_x: i32,
    pub step_block_y: i32,
    pub step_block_z: i32,
}

impl DensityVolume {
    pub const NO_BLOCK: i32 = -1;

    /// Panics on non-positive size/step (Java IllegalArgumentException).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        size_x: i32,
        size_y: i32,
        size_z: i32,
        min_block_x: i32,
        min_block_y: i32,
        min_block_z: i32,
        step_block_x: i32,
        step_block_y: i32,
        step_block_z: i32,
    ) -> DensityVolume {
        assert!(
            size_x > 0 && size_y > 0 && size_z > 0,
            "Size must be positive, was: {size_x}x{size_y}x{size_z}"
        );
        assert!(
            step_block_x > 0 && step_block_y > 0 && step_block_z > 0,
            "Step must be positive, was: {step_block_x}; {step_block_y}; {step_block_z}"
        );
        DensityVolume {
            size_x,
            size_y,
            size_z,
            min_block_x,
            min_block_y,
            min_block_z,
            step_block_x,
            step_block_y,
            step_block_z,
        }
    }

    /// Unit-step volume.
    pub fn with_unit_step(
        size_x: i32,
        size_y: i32,
        size_z: i32,
        min_block_x: i32,
        min_block_y: i32,
        min_block_z: i32,
    ) -> DensityVolume {
        Self::new(
            size_x,
            size_y,
            size_z,
            min_block_x,
            min_block_y,
            min_block_z,
            1,
            1,
            1,
        )
    }

    #[inline]
    pub fn index_unchecked(&self, index_x: i32, index_y: i32, index_z: i32) -> usize {
        (index_y + (index_x + index_z * self.size_x) * self.size_y) as usize
    }
    #[inline]
    pub fn block_x(&self, x: i32) -> i32 {
        self.min_block_x + x * self.step_block_x
    }
    #[inline]
    pub fn block_y(&self, y: i32) -> i32 {
        self.min_block_y + y * self.step_block_y
    }
    #[inline]
    pub fn block_z(&self, z: i32) -> i32 {
        self.min_block_z + z * self.step_block_z
    }
    pub fn max_block_x(&self) -> i32 {
        self.min_block_x + self.size_x * self.step_block_x - 1
    }
    pub fn max_block_y(&self) -> i32 {
        self.min_block_y + self.size_y * self.step_block_y - 1
    }
    pub fn max_block_z(&self) -> i32 {
        self.min_block_z + self.size_z * self.step_block_z - 1
    }
    pub fn size(&self) -> usize {
        (self.size_x * self.size_y * self.size_z) as usize
    }

    fn contains_block_relative(&self, rx: i32, ry: i32, rz: i32) -> bool {
        rx >= 0
            && ry >= 0
            && rz >= 0
            && rx < self.size_x * self.step_block_x
            && ry < self.size_y * self.step_block_y
            && rz < self.size_z * self.step_block_z
            && floor_mod(rx, self.step_block_x) == 0
            && floor_mod(ry, self.step_block_y) == 0
            && floor_mod(rz, self.step_block_z) == 0
    }

    /// Index of the grid point at exactly this block position, or `None` ([`Self::NO_BLOCK`] in vanilla).
    pub fn index_of_block(&self, block_x: i32, block_y: i32, block_z: i32) -> Option<usize> {
        let rx = block_x - self.min_block_x;
        let ry = block_y - self.min_block_y;
        let rz = block_z - self.min_block_z;
        if self.step_block_x == 1 && self.step_block_y == 1 && self.step_block_z == 1 {
            if rx >= 0
                && ry >= 0
                && rz >= 0
                && rx < self.size_x
                && ry < self.size_y
                && rz < self.size_z
            {
                return Some(self.index_unchecked(rx, ry, rz));
            }
        } else if self.contains_block_relative(rx, ry, rz) {
            return Some(self.index_unchecked(
                floor_div(rx, self.step_block_x),
                floor_div(ry, self.step_block_y),
                floor_div(rz, self.step_block_z),
            ));
        }
        None
    }
}

// ---------------------------------------------------------------------------------------------
// DensityBuffer
// ---------------------------------------------------------------------------------------------

/// Fixed-size float buffer (`DensityBuffer`); `size()` is its length.
#[derive(Clone, Debug, Default)]
pub struct DensityBuffer {
    values: Vec<f32>,
}

impl DensityBuffer {
    /// `DensityBuffer.createUnpooled(size)`: zero-filled.
    pub fn create_unpooled(size: usize) -> DensityBuffer {
        DensityBuffer {
            values: vec![0.0; size],
        }
    }
    #[inline]
    pub fn set(&mut self, index: usize, value: f32) {
        self.values[index] = value;
    }
    /// Sets `[index, index + size)` to `value`.
    #[inline]
    pub fn set_range(&mut self, index: usize, size: usize, value: f32) {
        self.values[index..index + size].fill(value);
    }
    #[inline]
    pub fn add_to(&mut self, index: usize, value: f32) {
        self.values[index] += value;
    }
    #[inline]
    pub fn get(&self, index: usize) -> f32 {
        self.values[index]
    }
    pub fn fill(&mut self, value: f32) {
        self.values.fill(value);
    }
    /// Panics on size mismatch (Java IllegalArgumentException).
    pub fn copy_from(&mut self, other: &DensityBuffer) {
        assert_eq!(
            self.size(),
            other.size(),
            "Cannot copy from buffer with size={}, expected {}",
            other.size(),
            self.size()
        );
        self.values.copy_from_slice(&other.values);
    }
    #[inline]
    pub fn size(&self) -> usize {
        self.values.len()
    }
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.values
    }
}

/// A buffer borrowed from the context's pool; returned to it on drop (vanilla: `ScopedDensityBuffer`).
pub struct ScopedDensityBuffer<'a> {
    buffer: DensityBuffer,
    pool: &'a RefCell<Vec<Vec<f32>>>,
}

impl Deref for ScopedDensityBuffer<'_> {
    type Target = DensityBuffer;
    fn deref(&self) -> &DensityBuffer {
        &self.buffer
    }
}
impl DerefMut for ScopedDensityBuffer<'_> {
    fn deref_mut(&mut self) -> &mut DensityBuffer {
        &mut self.buffer
    }
}
impl Drop for ScopedDensityBuffer<'_> {
    fn drop(&mut self) {
        let values = std::mem::take(&mut self.buffer.values);
        let mut pool = self.pool.borrow_mut();
        if pool.len() < MAX_POOLED_BUFFERS {
            pool.push(values);
        }
    }
}

const MAX_POOLED_BUFFERS: usize = 64;

// ---------------------------------------------------------------------------------------------
// ContextKey / ContextMap
// ---------------------------------------------------------------------------------------------

/// Typed key of a user field of a [`SamplerContext`] (`ContextKey<T>`), e.g. `"blender"` or `"beardifier"`.
pub struct ContextKey<T: ?Sized> {
    pub name: &'static str,
    _marker: PhantomData<fn() -> Box<T>>,
}

impl<T: ?Sized> ContextKey<T> {
    /// `ContextKey.vanilla(name)`.
    pub const fn vanilla(name: &'static str) -> ContextKey<T> {
        ContextKey {
            name,
            _marker: PhantomData,
        }
    }
}
impl<T: ?Sized> Clone for ContextKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ?Sized> Copy for ContextKey<T> {}

/// Immutable bag of user fields of a context (`ContextMap`).
#[derive(Default)]
pub struct ContextMap {
    fields: HashMap<&'static str, Box<dyn Any + Send + Sync>>,
}

impl ContextMap {
    pub fn new() -> ContextMap {
        ContextMap::default()
    }
    pub fn with<T: Send + Sync + 'static>(mut self, key: ContextKey<T>, value: T) -> ContextMap {
        self.fields.insert(key.name, Box::new(value));
        self
    }
    pub fn get<T: Send + Sync + 'static>(&self, key: &ContextKey<T>) -> Option<&T> {
        self.fields
            .get(key.name)
            .and_then(|v| v.downcast_ref::<T>())
    }
}

/// The `Blender` interface as seen by density functions (`blend_density`, `blend_alpha`, `blend_offset`).
/// The real implementation lives with chunk generation.
pub trait Blender: Send + Sync {
    fn is_empty(&self) -> bool;
    fn blend_density(&self, block_x: i32, block_y: i32, block_z: i32, input: f32) -> f32;
}

pub type SharedSampler = Arc<dyn DensitySampler>;
pub type SharedBlender = Arc<dyn Blender>;

/// `Blender.CONTEXT_KEY`.
pub const BLENDER_KEY: ContextKey<SharedBlender> = ContextKey::vanilla("blender");
/// `Blender.ALPHA_KEY`.
pub const BLENDER_ALPHA_KEY: ContextKey<SharedSampler> = ContextKey::vanilla("blender/alpha");
/// `Blender.OFFSET_KEY`.
pub const BLENDER_OFFSET_KEY: ContextKey<SharedSampler> = ContextKey::vanilla("blender/offset");
/// `Beardifier.CONTEXT_KEY` (the beardifier is exposed as a sampler).
pub const BEARDIFIER_KEY: ContextKey<SharedSampler> = ContextKey::vanilla("beardifier");

// ---------------------------------------------------------------------------------------------
// SamplerContext
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct CacheCell {
    volume: Option<DensityVolume>,
    buffer: Option<DensityBuffer>,
    value_key: i64,
    value: f32,
}

impl CacheCell {
    fn new() -> CacheCell {
        CacheCell {
            volume: None,
            buffer: None,
            value_key: 0,
            value: f32::NAN,
        }
    }
}

/// Per-sampling state (user fields, buffer pool, cache cells). One per thread/chunk; not `Sync`.
pub struct SamplerContext {
    user_fields: ContextMap,
    pool: RefCell<Vec<Vec<f32>>>,
    cache_cells: Option<RefCell<Vec<CacheCell>>>,
}

impl SamplerContext {
    /// `SamplerContext.EMPTY_UNCACHED` (no user fields, no caches).
    pub fn empty_uncached() -> SamplerContext {
        SamplerContext {
            user_fields: ContextMap::new(),
            pool: RefCell::new(Vec::new()),
            cache_cells: None,
        }
    }
    pub fn builder() -> SamplerContextBuilder {
        SamplerContextBuilder {
            user_fields: ContextMap::new(),
            enable_caches: false,
        }
    }
    pub fn get_field<T: Send + Sync + 'static>(&self, key: &ContextKey<T>) -> Option<&T> {
        self.user_fields.get(key)
    }

    /// Borrows a buffer of `volume.size()` floats from the pool.
    pub fn acquire_buffer(&self, volume: &DensityVolume) -> ScopedDensityBuffer<'_> {
        self.acquire_buffer_sized(volume.size())
    }
    pub fn acquire_buffer_sized(&self, size: usize) -> ScopedDensityBuffer<'_> {
        let mut values = self.pool.borrow_mut().pop().unwrap_or_default();
        values.clear();
        values.resize(size, 0.0);
        ScopedDensityBuffer {
            buffer: DensityBuffer { values },
            pool: &self.pool,
        }
    }

    /// `SamplerContext.sampleVolumeCached`.
    pub fn sample_volume_cached(
        &self,
        cache_id: usize,
        input: &dyn DensitySampler,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        let Some(cells) = &self.cache_cells else {
            input.sample_volume(self, out, volume);
            return;
        };
        let stale = {
            let mut cells = cells.borrow_mut();
            let cell = Self::cell(&mut cells, cache_id);
            cell.buffer.is_none() || cell.volume.as_ref() != Some(volume)
        };
        if stale {
            // The input is evaluated without holding the cell borrow (it may sample other cache ids).
            let mut buffer = DensityBuffer {
                values: self.pool.borrow_mut().pop().unwrap_or_default(),
            };
            buffer.values.clear();
            buffer.values.resize(volume.size(), 0.0);
            input.sample_volume(self, &mut buffer, volume);
            let mut cells = cells.borrow_mut();
            let cell = Self::cell(&mut cells, cache_id);
            if let Some(old) = cell.buffer.replace(buffer) {
                self.release(old);
            }
            cell.volume = Some(*volume);
        }
        let cells = cells.borrow();
        out.copy_from(
            cells[cache_id]
                .buffer
                .as_ref()
                .expect("cache buffer present"),
        );
    }

    /// `SamplerContext.sampleValueCached`.
    pub fn sample_value_cached(
        &self,
        cache_id: usize,
        input: &dyn DensitySampler,
        block_x: i32,
        block_y: i32,
        block_z: i32,
    ) -> f32 {
        let Some(cells) = &self.cache_cells else {
            return input.sample_value(self, block_x, block_y, block_z);
        };
        let cache_key = pack_block_pos(block_x, block_y, block_z);
        {
            let mut cells = cells.borrow_mut();
            let cell = Self::cell(&mut cells, cache_id);
            if cell.value_key == cache_key && !cell.value.is_nan() {
                return cell.value;
            }
            if let (Some(buffer), Some(volume)) = (&cell.buffer, &cell.volume)
                && let Some(index) = volume.index_of_block(block_x, block_y, block_z)
            {
                return buffer.get(index);
            }
        }
        let value = input.sample_value(self, block_x, block_y, block_z);
        let mut cells = cells.borrow_mut();
        let cell = Self::cell(&mut cells, cache_id);
        cell.value_key = cache_key;
        cell.value = value;
        value
    }

    fn release(&self, buffer: DensityBuffer) {
        let mut pool = self.pool.borrow_mut();
        if pool.len() < MAX_POOLED_BUFFERS {
            pool.push(buffer.values);
        }
    }

    fn cell(cells: &mut Vec<CacheCell>, id: usize) -> &mut CacheCell {
        if id >= cells.len() {
            // Vanilla grows in steps of 16 (Mth.roundToward(id + 1, 16)).
            let new_size = (id + 1).div_ceil(16) * 16;
            cells.resize_with(new_size, CacheCell::new);
        }
        &mut cells[id]
    }
}

/// `BlockPos.asLong(x, y, z)`.
#[inline]
pub fn pack_block_pos(x: i32, y: i32, z: i32) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

pub struct SamplerContextBuilder {
    user_fields: ContextMap,
    enable_caches: bool,
}

impl SamplerContextBuilder {
    pub fn set_user_fields(mut self, user_fields: ContextMap) -> Self {
        self.user_fields = user_fields;
        self
    }
    pub fn enable_caches(mut self) -> Self {
        self.enable_caches = true;
        self
    }
    pub fn build(self) -> SamplerContext {
        SamplerContext {
            user_fields: self.user_fields,
            pool: RefCell::new(Vec::new()),
            cache_cells: self.enable_caches.then(|| RefCell::new(Vec::new())),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// CachingDensitySampler / ContextBoundSampler
// ---------------------------------------------------------------------------------------------

/// `CachingDensitySampler`: routes through the context's cache cell `id`.
pub struct CachingDensitySampler {
    pub id: usize,
    pub input: SharedSampler,
}

impl DensitySampler for CachingDensitySampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        context.sample_volume_cached(self.id, &*self.input, out, volume);
    }
    fn sample_value(&self, context: &SamplerContext, x: i32, y: i32, z: i32) -> f32 {
        context.sample_value_cached(self.id, &*self.input, x, y, z)
    }
}

/// `ContextBoundSampler`: uses the sampler bound to `key` in the context, else the fallback.
pub struct ContextBoundSampler {
    pub key: ContextKey<SharedSampler>,
    pub fallback_sampler: SharedSampler,
}

impl ContextBoundSampler {
    fn resolve<'a>(&'a self, context: &'a SamplerContext) -> &'a dyn DensitySampler {
        match context.get_field(&self.key) {
            Some(s) => &**s,
            None => &*self.fallback_sampler,
        }
    }
}

impl DensitySampler for ContextBoundSampler {
    fn sample_volume(
        &self,
        context: &SamplerContext,
        out: &mut DensityBuffer,
        volume: &DensityVolume,
    ) {
        self.resolve(context).sample_volume(context, out, volume);
    }
    fn sample_value(&self, context: &SamplerContext, x: i32, y: i32, z: i32) -> f32 {
        self.resolve(context).sample_value(context, x, y, z)
    }
}
