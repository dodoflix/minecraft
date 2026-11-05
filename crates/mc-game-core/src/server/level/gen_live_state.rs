//! What chunk generation (worker threads, no `ServerLevel`) reads from the live level: vanilla's `WorldGenRegion` reaches
//! the real level through `getLevel()` for the difficulty, the `spawnMobs` game rule, the clocks and the registries when
//! it spawns the original mobs of a chunk. The level publishes these every tick ([`GenLiveState::publish`]) and once at
//! creation.

use crate::registry::RegistryAccess;
use crate::registry::typed::dimension_type::DimensionType;
use mc_world::level_storage::Difficulty;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

pub struct GenLiveState {
    difficulty: AtomicI32,
    spawn_mobs: AtomicBool,
    overworld_clock_time: AtomicI64,
    moon_brightness: AtomicU32,
    registries: OnceLock<RegistryAccess>,
    dimension_type: OnceLock<Arc<DimensionType>>,
}

impl Default for GenLiveState {
    fn default() -> Self {
        GenLiveState {
            difficulty: AtomicI32::new(Difficulty::Normal.get_id()),
            spawn_mobs: AtomicBool::new(true),
            overworld_clock_time: AtomicI64::new(0),
            moon_brightness: AtomicU32::new(0.0f32.to_bits()),
            registries: OnceLock::new(),
            dimension_type: OnceLock::new(),
        }
    }
}

impl GenLiveState {
    /// Publishes the values of the current tick.
    pub fn publish(&self, difficulty: Difficulty, spawn_mobs: bool, overworld_clock_time: i64, moon_brightness: f32) {
        self.difficulty.store(difficulty.get_id(), Ordering::Relaxed);
        self.spawn_mobs.store(spawn_mobs, Ordering::Relaxed);
        self.overworld_clock_time.store(overworld_clock_time, Ordering::Relaxed);
        self.moon_brightness.store(moon_brightness.to_bits(), Ordering::Relaxed);
    }

    /// Installed once by the level factory.
    pub fn install(&self, registries: RegistryAccess, dimension_type: Arc<DimensionType>) {
        let _ = self.registries.set(registries);
        let _ = self.dimension_type.set(dimension_type);
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

    /// `registryAccess()` (empty before the factory installed it).
    pub fn registries(&self) -> &RegistryAccess {
        static EMPTY: std::sync::LazyLock<RegistryAccess> = std::sync::LazyLock::new(RegistryAccess::default);
        self.registries.get().unwrap_or(&EMPTY)
    }

    pub fn dimension_type(&self) -> Option<&Arc<DimensionType>> {
        self.dimension_type.get()
    }
}
