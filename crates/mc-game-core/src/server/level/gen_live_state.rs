//! What chunk generation (worker threads, no `ServerLevel`) reads from the live level: vanilla's `WorldGenRegion` reaches
//! the real level through `getLevel()` for the difficulty, the `spawnMobs` game rule, the clocks and the registries when
//! it spawns the original mobs of a chunk, and the world border through `getWorldBorder()` (the spawn placement types
//! check it). The level publishes these every tick ([`GenLiveState::publish`], [`GenLiveState::publish_border`]) and once
//! at creation.

use crate::registry::RegistryAccess;
use crate::registry::typed::dimension_type::DimensionType;
use mc_core::pos::BlockPos;
use mc_world::level_storage::Difficulty;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

pub struct GenLiveState {
    difficulty: AtomicI32,
    spawn_mobs: AtomicBool,
    overworld_clock_time: AtomicI64,
    moon_brightness: AtomicU32,
    /// The bounds of the world border (`getMinX()`, `getMaxX()`, `getMinZ()`, `getMaxZ()`) as `f64` bits.
    border: [AtomicU64; 4],
    /// `registryAccess()`; `/reload` swaps it.
    registries: RwLock<RegistryAccess>,
    dimension_type: OnceLock<Arc<DimensionType>>,
    /// The level's private `ENTITY_COUNTER` (see `ServerLevel::entity_id_counter`), when it has one.
    entity_counter: OnceLock<Arc<AtomicI32>>,
    /// The light engine's published data (`getLightEngine()` of the level), installed by the chunk map.
    light: OnceLock<mc_light::LightReader>,
}

impl Default for GenLiveState {
    fn default() -> Self {
        GenLiveState {
            difficulty: AtomicI32::new(Difficulty::Normal.get_id()),
            spawn_mobs: AtomicBool::new(true),
            overworld_clock_time: AtomicI64::new(0),
            moon_brightness: AtomicU32::new(0.0f32.to_bits()),
            border: [
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
            ]
            .map(|bound| AtomicU64::new(bound.to_bits())),
            registries: RwLock::new(RegistryAccess::default()),
            dimension_type: OnceLock::new(),
            entity_counter: OnceLock::new(),
            light: OnceLock::new(),
        }
    }
}

impl GenLiveState {
    /// Publishes the values of the current tick.
    pub fn publish(
        &self,
        difficulty: Difficulty,
        spawn_mobs: bool,
        overworld_clock_time: i64,
        moon_brightness: f32,
    ) {
        self.difficulty
            .store(difficulty.get_id(), Ordering::Relaxed);
        self.spawn_mobs.store(spawn_mobs, Ordering::Relaxed);
        self.overworld_clock_time
            .store(overworld_clock_time, Ordering::Relaxed);
        self.moon_brightness
            .store(moon_brightness.to_bits(), Ordering::Relaxed);
    }

    /// Publishes the bounds of the level's world border (`[min_x, max_x, min_z, max_z]`).
    pub fn publish_border(&self, bounds: [f64; 4]) {
        for (slot, bound) in self.border.iter().zip(bounds) {
            slot.store(bound.to_bits(), Ordering::Relaxed);
        }
    }

    /// `getWorldBorder().isWithinBounds(pos)` as of the last publication.
    pub fn is_within_world_border(&self, pos: BlockPos) -> bool {
        let [min_x, max_x, min_z, max_z] = self
            .border
            .each_ref()
            .map(|b| f64::from_bits(b.load(Ordering::Relaxed)));
        let (x, z) = (f64::from(pos.x()), f64::from(pos.z()));
        x >= min_x && x < max_x && z >= min_z && z < max_z
    }

    /// Installed once by the level factory.
    pub fn install(&self, registries: RegistryAccess, dimension_type: Arc<DimensionType>) {
        self.set_registries(registries);
        let _ = self.dimension_type.set(dimension_type);
    }

    /// Installed once by the chunk map.
    pub fn install_light(&self, light: mc_light::LightReader) {
        let _ = self.light.set(light);
    }

    pub fn light(&self) -> Option<&mc_light::LightReader> {
        self.light.get()
    }

    /// Makes generation take its entity ids from the level's private counter.
    pub fn share_entity_counter(&self, counter: Arc<AtomicI32>) {
        let _ = self.entity_counter.set(counter);
    }

    /// `ENTITY_COUNTER.incrementAndGet()` of the level's private counter, `None` when the level uses the process wide one.
    pub fn next_entity_id(&self) -> Option<i32> {
        self.entity_counter
            .get()
            .map(|c| c.fetch_add(1, Ordering::Relaxed).wrapping_add(1))
    }

    pub fn difficulty(&self) -> Difficulty {
        Difficulty::by_id(self.difficulty.load(Ordering::Relaxed))
    }

    pub fn spawn_mobs(&self) -> bool {
        self.spawn_mobs.load(Ordering::Relaxed)
    }

    pub fn overworld_clock_time(&self) -> i64 {
        self.overworld_clock_time.load(Ordering::Relaxed)
    }

    pub fn moon_brightness(&self) -> f32 {
        f32::from_bits(self.moon_brightness.load(Ordering::Relaxed))
    }

    /// What `/reload` does to `server.registryAccess()`: generation reads the reloaded registries and tags from now on.
    pub fn set_registries(&self, registries: RegistryAccess) {
        *self.registries.write().unwrap_or_else(|e| e.into_inner()) = registries;
    }

    /// `registryAccess()` (empty before the factory installed it).
    pub fn registries(&self) -> RegistryAccess {
        self.registries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn dimension_type(&self) -> Option<&Arc<DimensionType>> {
        self.dimension_type.get()
    }
}
