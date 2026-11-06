//! Mob spawning infrastructure (`NaturalSpawner`, `SpawnPlacements`, `MobSpawnSettings`, the `CustomSpawner` driver).
//!
//! # For mob authors (phase 2)
//!
//! * **Spawn predicate of a type.** Vanilla's `SpawnPlacements.register(type, placement, heightmap, predicate)`: the
//!   placement type and heightmap are `mc_data` data already, the predicate is code. Types whose vanilla predicate is a
//!   shared static (`Mob/Animal/Monster/WaterAnimal/AgeableWaterCreature`) are bound in
//!   `world::entity::spawn_placements`; every other type registers its own from its module:
//!   ```ignore
//!   fn check_ocelot_spawn_rules(ty: EntityType, level: &dyn ServerLevelAccessor, reason: EntitySpawnReason, pos: BlockPos, random: &mut dyn RandomSource) -> bool { .. }
//!   register_spawn_placement!(EntityType::OCELOT, check_ocelot_spawn_rules);
//!   ```
//!   `spawn_placements::missing_predicates()` lists the types still unregistered. Shared statics to call from your own
//!   predicates: `world::entity::spawn_predicates` (`check_monster_spawn_rules`, `is_dark_enough_to_spawn`, ...).
//! * **Entity hooks** (`Entity` trait, spawn framework section; `Mob` and subclasses override): `is_mob`,
//!   `is_persistence_required`, `requires_custom_persistence`, `check_spawn_rules(level, reason)`,
//!   `check_spawn_obstruction(level)`, `get_max_spawn_cluster_size`, `is_max_group_size_reached(n)`,
//!   `remove_when_far_away(dist_sqr)` and `finalize_spawn(level, difficulty, reason, group_data)`. `level` is a
//!   [`ServerLevelAccessor`](crate::world::level::server_level_accessor::ServerLevelAccessor): implemented by `ServerLevel`
//!   and by the generation region (`GenLevel`), so the same code runs for natural spawns and chunk-generation spawns.
//!   Group data: `world::entity::spawn_group_data::SpawnGroupData` (`impl_spawn_group_data!`).
//! * **Creating mobs**: `factory::create(type, &level, request)` (checks `EntityType.canSpawn`) or, off the game thread,
//!   `ServerLevelAccessor::create_entity`. A registration's `create` gets `(type, entity id, registries)`.
//! * **Custom spawners** (`PhantomSpawner`, `PatrolSpawner`, `CatSpawner`, `VillageSiege`, `WanderingTraderSpawner`):
//!   implement [`custom_spawner::CustomSpawner`] and register a [`custom_spawner::CustomSpawnerRegistration`] in
//!   `CUSTOM_SPAWNERS` (its `create` returns `None` for levels that do not get the spawner, e.g. non-overworld
//!   dimensions); `ServerLevel::tick_custom_spawners` runs them in `order` after the chunk ticks while `spawnMobs` is on.
//! * **Biome spawn lists / costs**: `ServerLevelAccessor::natural_mob_spawns_at(pos)` ->
//!   [`mob_spawn_settings::MobSpawnSettings`]; structure `spawn_overrides`: `StructureSettings::spawn_override`.
//! * **Despawning**: `MobCategoryExt` (`get_despawn_distance`, `get_no_despawn_distance`, ...) for `Mob.checkDespawn`.

pub mod biome_spawns;
pub mod chunk_gen;
pub mod custom_spawner;
pub mod local_mob_cap_calculator;
pub mod mob_category;
pub mod mob_spawn_settings;
pub mod natural_spawner;
pub mod potential_calculator;

pub use mob_category::MobCategoryExt;

use crate::registry::typed::dimension_type::DimensionType;
use crate::server::level::gen_live_state::GenLiveState;
use biome_spawns::{BiomeSpawnTable, DimensionSpawnBase};
use custom_spawner::CustomSpawner;
use mc_data::enums::MobCategory;
use mc_worldgen::biome::BiomeRegistry;
use natural_spawner::SpawnState;
use std::sync::{Arc, OnceLock};

/// The per-level state of natural spawning (`ServerChunkCache.lastSpawnState`, `ServerLevel.customSpawners`, the
/// dimension layer of the spawning attributes, the values worldgen threads read).
#[derive(Default)]
pub struct LevelSpawnData {
    /// What chunk generation reads from the live level.
    pub gen_live: Arc<GenLiveState>,
    /// `ServerLevel.customSpawners`.
    pub custom_spawners: Vec<Box<dyn CustomSpawner>>,
    /// `dimensionType()` (set by the level factory).
    pub dimension_type: Option<Arc<DimensionType>>,
    /// The dimension type's constant layer of `natural_mob_spawns` / `creature_world_gen_spawn_probability`.
    pub dimension_base: DimensionSpawnBase,
    /// `getOverworldClockTime()` and `getMoonBrightness(pos)` as of the start of this tick (`getCurrentDifficultyAt`).
    pub overworld_clock_time: i64,
    pub moon_brightness: f32,
    /// `ServerChunkCache.lastSpawnState` + the categories that may spawn this tick.
    spawn_state: Option<(SpawnState, Vec<MobCategory>)>,
    biome_table: OnceLock<Arc<BiomeSpawnTable>>,
}

impl LevelSpawnData {
    /// Installs the dimension type (level factory).
    pub fn install_dimension_type(&mut self, dimension_type: Arc<DimensionType>) {
        self.dimension_base = DimensionSpawnBase::from_attributes(&dimension_type.attributes);
        self.dimension_type = Some(dimension_type);
    }

    /// `lastSpawnState = state` + this tick's spawning categories.
    pub fn set_spawn_state(&mut self, state: SpawnState, categories: Vec<MobCategory>) {
        self.spawn_state = Some((state, categories));
    }

    /// Whether any category may spawn this tick (`!spawningCategories.isEmpty()`).
    pub fn has_spawning_categories(&self) -> bool {
        self.spawn_state
            .as_ref()
            .is_some_and(|(_, categories)| !categories.is_empty())
    }

    /// `getLastSpawnState()`.
    pub fn last_spawn_state(&self) -> Option<&SpawnState> {
        self.spawn_state.as_ref().map(|(state, _)| state)
    }

    pub(crate) fn take_spawn_state(&mut self) -> Option<(SpawnState, Vec<MobCategory>)> {
        self.spawn_state.take()
    }

    pub(crate) fn restore_spawn_state(&mut self, state: SpawnState, categories: Vec<MobCategory>) {
        self.spawn_state = Some((state, categories));
    }

    /// The decoded spawning attributes of the biomes of `registry`.
    pub fn biome_table(
        &self,
        registry: Option<&Arc<BiomeRegistry>>,
    ) -> Option<&Arc<BiomeSpawnTable>> {
        match self.biome_table.get() {
            Some(table) => Some(table),
            None => {
                let table = BiomeSpawnTable::for_registry(registry?);
                Some(self.biome_table.get_or_init(|| table))
            }
        }
    }
}
