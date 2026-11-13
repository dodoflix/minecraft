//! `net.minecraft.world.level.NaturalSpawner`: the per-tick natural mob spawning of loaded chunks
//! (`createState` / `getFilteredSpawningCategories` / `spawnForChunk`) and the original mob spawns of chunk generation
//! (`spawnMobsForChunkGeneration`). RNG call order, the cluster/group loops and every distance test are vanilla's.

use super::local_mob_cap_calculator::LocalMobCapCalculator;
use super::mob_category::{MobCategoryExt, SPAWNING_CATEGORIES};
use super::mob_spawn_settings::{MobSpawnSettings, list_contains};
use super::potential_calculator::PotentialCalculator;
use crate::server::level::ServerLevel;
use crate::world::entity::dismount_helper;
use crate::world::entity::factory::EntitySpawnRequest;
use crate::world::entity::spawn_group_data::SpawnGroupData;
use crate::world::entity::spawn_placements::{self, SpawnPlacementTypeExt as _};
use crate::world::level::block::BlockStateExt as _;
use crate::world::level::levelgen::structure::manager::StructureDataSource;
use crate::world::level::levelgen::structure::structure::SpawnerData;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::{BlockGetter, LevelReader as _};
use mc_core::math::mth;
use mc_core::phys::{AABB, Vec3};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_data::block::Block;
use mc_data::enums::{EntitySpawnReason, MobCategory};
use mc_data::fluid::FluidState;
use mc_world::heightmap::HeightmapType;
use mc_worldgen::provider::WeightedList;
use std::sync::{Arc, LazyLock};

/// `MIN_SPAWN_DISTANCE`.
pub const MIN_SPAWN_DISTANCE: i32 = 24;
/// `SPAWN_DISTANCE_CHUNK`.
pub const SPAWN_DISTANCE_CHUNK: i32 = 8;
/// `SPAWN_DISTANCE_BLOCK`.
pub const SPAWN_DISTANCE_BLOCK: i32 = 128;
/// `MAGIC_NUMBER = (int) Math.pow(17.0, 2.0)`.
const MAGIC_NUMBER: i32 = 289;

/// `INSCRIBED_SQUARE_SPAWN_DISTANCE_CHUNK = Mth.floor(8.0F / Mth.SQRT_OF_TWO)`.
pub fn inscribed_square_spawn_distance_chunk() -> i32 {
    mth::floor_f32(8.0f32 / mth::SQRT_OF_TWO)
}

/// `NaturalSpawner.SpawnPredicate` + `AfterSpawnCallback` of one spawn pass: [`SpawnState`] for the server tick,
/// [`NoSpawnHooks`] for debug spawns.
pub trait SpawnHooks {
    /// `SpawnPredicate.test(type, level, pos, chunk)`.
    fn can_spawn(&mut self, ty: EntityType, level: &dyn ServerLevelAccessor, pos: BlockPos)
    -> bool;
    /// `AfterSpawnCallback.run(mob, chunk)` (`ty` and `pos` are the spawned mob's type and block position).
    fn after_spawn(&mut self, level: &mut ServerLevel, ty: EntityType, pos: BlockPos);
}

/// `(type, level, pos, chunk) -> true` / `(mob, chunk) -> {}`.
pub struct NoSpawnHooks;

impl SpawnHooks for NoSpawnHooks {
    fn can_spawn(
        &mut self,
        _ty: EntityType,
        _level: &dyn ServerLevelAccessor,
        _pos: BlockPos,
    ) -> bool {
        true
    }
    fn after_spawn(&mut self, _level: &mut ServerLevel, _ty: EntityType, _pos: BlockPos) {}
}

/// `NaturalSpawner.SpawnState`.
pub struct SpawnState {
    spawnable_chunk_count: i32,
    mob_category_counts: [i32; 8],
    spawn_potential: PotentialCalculator,
    local_mob_cap_calculator: LocalMobCapCalculator,
    last_checked_pos: Option<BlockPos>,
    last_checked_type: Option<EntityType>,
    last_charge: f64,
}

impl SpawnState {
    /// `getSpawnableChunkCount()`.
    pub fn get_spawnable_chunk_count(&self) -> i32 {
        self.spawnable_chunk_count
    }

    /// The point charges of the spawn potential (`spawnPotential.charges`), in insertion order.
    pub fn spawn_potential_charges(&self) -> Vec<(BlockPos, f64)> {
        self.spawn_potential.charges()
    }

    /// `getMobCategoryCounts()`: the count of `category`.
    pub fn get_mob_category_count(&self, category: MobCategory) -> i32 {
        self.mob_category_counts[category as usize]
    }

    /// `canSpawnForCategoryGlobal(category)`.
    pub fn can_spawn_for_category_global(&self, category: MobCategory) -> bool {
        let max_mob_count = category
            .get_max_instances_per_chunk()
            .wrapping_mul(self.spawnable_chunk_count)
            / MAGIC_NUMBER;
        self.mob_category_counts[category as usize] < max_mob_count
    }

    /// `canSpawnForCategoryLocal(category, chunkPos)`.
    pub fn can_spawn_for_category_local(
        &mut self,
        chunk_map: &mut crate::server::level::ChunkMap,
        category: MobCategory,
        chunk_pos: ChunkPos,
    ) -> bool {
        self.local_mob_cap_calculator
            .can_spawn(chunk_map, category, chunk_pos)
    }

    /// `getPotentialEnergyChange`-based part of `canSpawn(type, level, testPos, chunk)`.
    fn charge_of(
        level: &dyn ServerLevelAccessor,
        pos: BlockPos,
        ty: EntityType,
    ) -> Option<(f64, f64)> {
        level
            .natural_mob_spawns_at(pos)
            .get_mob_spawn_cost(ty)
            .map(|cost| (cost.charge, cost.energy_budget))
    }
}

impl SpawnHooks for SpawnState {
    /// `SpawnState.canSpawn`.
    fn can_spawn(
        &mut self,
        ty: EntityType,
        level: &dyn ServerLevelAccessor,
        test_pos: BlockPos,
    ) -> bool {
        self.last_checked_pos = Some(test_pos);
        self.last_checked_type = Some(ty);
        match Self::charge_of(level, test_pos, ty) {
            None => {
                self.last_charge = 0.0;
                true
            }
            Some((charge, energy_budget)) => {
                self.last_charge = charge;
                let energy_change = self
                    .spawn_potential
                    .get_potential_energy_change(test_pos, charge);
                energy_change <= energy_budget
            }
        }
    }

    /// `SpawnState.afterSpawn`.
    fn after_spawn(&mut self, level: &mut ServerLevel, ty: EntityType, pos: BlockPos) {
        let charge = if self.last_checked_pos == Some(pos) && self.last_checked_type == Some(ty) {
            self.last_charge
        } else {
            Self::charge_of(level, pos, ty).map_or(0.0, |(charge, _)| charge)
        };
        self.spawn_potential.add_charge(pos, charge);
        let category = ty.data().category;
        self.mob_category_counts[category as usize] += 1;
        self.local_mob_cap_calculator.add_mob(
            &mut level.chunk_source.chunk_map,
            ChunkPos::containing(&pos),
            category,
        );
    }
}

/// `NaturalSpawner.createState(spawnableChunkCount, level, chunkGetter, localMobCapCalculator)` with the chunk getter of
/// `ServerChunkCache` (`getFullChunk`: only loaded full chunks count).
pub fn create_state(spawnable_chunk_count: i32, level: &mut ServerLevel) -> SpawnState {
    struct Counted {
        ty: EntityType,
        pos: BlockPos,
        is_mob: bool,
    }
    let counted: Vec<Counted> = level
        .entity_manager
        .get_all()
        .filter(|entity| {
            !(entity.is_mob()
                && (entity.is_persistence_required() || entity.requires_custom_persistence()))
        })
        .filter(|entity| entity.get_type().data().category != MobCategory::Misc)
        .map(|entity| Counted {
            ty: entity.get_type(),
            pos: entity.block_position(),
            is_mob: entity.is_mob(),
        })
        .collect();
    let mut spawn_potential = PotentialCalculator::new();
    let mut mob_category_counts = [0i32; 8];
    let mut local_mob_cap_calculator = LocalMobCapCalculator::new();
    for entity in counted {
        if level
            .chunk_now(entity.pos.x() >> 4, entity.pos.z() >> 4)
            .is_none()
        {
            continue;
        }
        let category = entity.ty.data().category;
        if let Some(cost) = level
            .natural_mob_spawns_at(entity.pos)
            .get_mob_spawn_cost(entity.ty)
        {
            spawn_potential.add_charge(entity.pos, cost.charge);
        }
        if entity.is_mob {
            local_mob_cap_calculator.add_mob(
                &mut level.chunk_source.chunk_map,
                ChunkPos::containing(&entity.pos),
                category,
            );
        }
        mob_category_counts[category as usize] += 1;
    }
    SpawnState {
        spawnable_chunk_count,
        mob_category_counts,
        spawn_potential,
        local_mob_cap_calculator,
        last_checked_pos: None,
        last_checked_type: None,
        last_charge: 0.0,
    }
}

/// `getFilteredSpawningCategories(state, spawnEnemies, spawnPersistent)`.
pub fn get_filtered_spawning_categories(
    state: &SpawnState,
    spawn_enemies: bool,
    spawn_persistent: bool,
) -> Vec<MobCategory> {
    let mut categories = Vec::with_capacity(SPAWNING_CATEGORIES.len());
    for category in SPAWNING_CATEGORIES {
        if (spawn_enemies || category.is_friendly())
            && (spawn_persistent || !category.is_persistent())
            && state.can_spawn_for_category_global(category)
        {
            categories.push(category);
        }
    }
    categories
}

/// `spawnForChunk(level, chunk, state, spawningCategories)` with the state and categories of this tick.
pub fn spawn_for_chunk(level: &mut ServerLevel, chunk_pos: ChunkPos) {
    let Some((mut state, categories)) = level.spawning.take_spawn_state() else {
        return;
    };
    for &category in &categories {
        if state.can_spawn_for_category_local(
            &mut level.chunk_source.chunk_map,
            category,
            chunk_pos,
        ) {
            spawn_category_for_chunk(category, level, chunk_pos, &mut state);
        }
    }
    level.spawning.restore_spawn_state(state, categories);
}

/// `spawnCategoryForChunk(category, level, chunk, extraTest, spawnCallback)`.
pub fn spawn_category_for_chunk(
    category: MobCategory,
    level: &mut ServerLevel,
    chunk_pos: ChunkPos,
    hooks: &mut dyn SpawnHooks,
) {
    let start = get_random_pos_within(level, chunk_pos);
    if start.y() > level.get_min_y() {
        spawn_category_for_position(category, level, chunk_pos, start, hooks);
    }
}

/// `spawnCategoryForPosition(category, level, start)` (debug: no extra test, no callback).
pub fn spawn_category_for_position_debug(
    category: MobCategory,
    level: &mut ServerLevel,
    start: BlockPos,
) {
    spawn_category_for_position(
        category,
        level,
        ChunkPos::containing(&start),
        start,
        &mut NoSpawnHooks,
    );
}

/// `(double)` squared despawn distance (`getDespawnDistance() * getDespawnDistance()` is an `int` product).
fn despawn_distance_sqr(category: MobCategory) -> f64 {
    let distance = category.get_despawn_distance();
    f64::from(distance.wrapping_mul(distance))
}

/// `spawnCategoryForPosition(category, level, chunk, start, extraTest, spawnCallback)`.
pub fn spawn_category_for_position(
    category: MobCategory,
    level: &mut ServerLevel,
    chunk_pos: ChunkPos,
    start: BlockPos,
    hooks: &mut dyn SpawnHooks,
) {
    let y_start = start.y();
    let state = BlockGetter::get_block_state(&*level, start);
    if state.is_redstone_conductor() {
        return;
    }
    let mut cluster_size = 0;
    for _group_count in 0..3 {
        let mut x = start.x();
        let mut z = start.z();
        let mut current_spawn_data: Option<SpawnerData> = None;
        let mut group_data: Option<Box<dyn SpawnGroupData>> = None;
        let mut max = mth::ceil_f32(level.tick_state.random.next_float() * 4.0f32);
        let mut group_size = 0;
        let mut ll = 0;
        while ll < max {
            x = x.wrapping_add(
                level.tick_state.random.next_int_bounded(6)
                    - level.tick_state.random.next_int_bounded(6),
            );
            z = z.wrapping_add(
                level.tick_state.random.next_int_bounded(6)
                    - level.tick_state.random.next_int_bounded(6),
            );
            let pos = BlockPos::new(x, y_start, z);
            let xx = f64::from(x) + 0.5;
            let zz = f64::from(z) + 0.5;
            if let Some(nearest_player) =
                level.get_nearest_player(xx, f64::from(y_start), zz, -1.0, |player| {
                    !player.is_spectator()
                })
            {
                let nearest_player_distance_sqr = level
                    .entity_manager
                    .get_any(nearest_player)
                    .map_or(f64::MAX, |p| {
                        p.base().distance_to_sqr_xyz(xx, f64::from(y_start), zz)
                    });
                if is_right_distance_to_player_and_spawn_point(
                    level,
                    chunk_pos,
                    pos,
                    nearest_player_distance_sqr,
                ) {
                    if current_spawn_data.is_none() {
                        match get_random_spawn_mob_at(level, category, pos) {
                            None => break,
                            Some(next) => {
                                max = next.count.sample(&mut level.tick_state.random);
                                current_spawn_data = Some(next);
                            }
                        }
                    }
                    let spawn_data = current_spawn_data.as_ref().expect("set above");
                    let ty = spawn_data.entity_type;
                    if is_valid_spawn_position_for_type(
                        level,
                        category,
                        spawn_data,
                        pos,
                        nearest_player_distance_sqr,
                    ) && hooks.can_spawn(ty, level, pos)
                    {
                        let Some(mut mob) = get_mob_for_spawn(level, ty) else {
                            return;
                        };
                        let y_rot = level.tick_state.random.next_float() * 360.0f32;
                        mob.snap_to(xx, f64::from(y_start), zz, y_rot, 0.0);
                        if is_valid_position_for_mob(level, &*mob, nearest_player_distance_sqr) {
                            let mob_pos = mob.block_position();
                            let difficulty =
                                ServerLevelAccessor::get_current_difficulty_at(&*level, mob_pos);
                            group_data = mob.finalize_spawn(
                                level,
                                difficulty,
                                EntitySpawnReason::Natural,
                                group_data,
                            );
                            cluster_size += 1;
                            group_size += 1;
                            let mob_pos = mob.block_position();
                            let max_cluster_size = mob.get_max_spawn_cluster_size();
                            let max_group_size_reached = mob.is_max_group_size_reached(group_size);
                            level.add_fresh_entity_with_passengers(vec![mob]);
                            hooks.after_spawn(level, ty, mob_pos);
                            if cluster_size >= max_cluster_size {
                                return;
                            }
                            if max_group_size_reached {
                                break;
                            }
                        }
                    }
                }
            }
            ll += 1;
        }
    }
}

/// `isRightDistanceToPlayerAndSpawnPoint(level, chunk, pos, nearestPlayerDistanceSqr)`.
fn is_right_distance_to_player_and_spawn_point(
    level: &ServerLevel,
    chunk_pos: ChunkPos,
    pos: BlockPos,
    nearest_player_distance_sqr: f64,
) -> bool {
    if nearest_player_distance_sqr <= 576.0 {
        return false;
    }
    let respawn_data = level.level_data.get_respawn_data();
    if respawn_data.dimension == level.dimension
        && respawn_data.pos.closer_to_center_than(
            &Vec3::new(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()),
                f64::from(pos.z()) + 0.5,
            ),
            24.0,
        )
    {
        return false;
    }
    let pos_chunk = ChunkPos::containing(&pos);
    pos_chunk == chunk_pos || level.can_spawn_entities_in_chunk(pos_chunk)
}

/// `EntityType.getSpawnAABB(x, y, z)`.
pub fn get_spawn_aabb(ty: EntityType, x: f64, y: f64, z: f64) -> AABB {
    let data = ty.data();
    let half_width = data.spawn_dimensions_scale * data.width / 2.0f32;
    let height = data.spawn_dimensions_scale * data.height;
    AABB::new(
        x - f64::from(half_width),
        y,
        z - f64::from(half_width),
        x + f64::from(half_width),
        y + f64::from(height),
        z + f64::from(half_width),
    )
}

/// `isValidSpawnPostitionForType(...)`.
fn is_valid_spawn_position_for_type(
    level: &mut ServerLevel,
    category: MobCategory,
    spawn_data: &SpawnerData,
    pos: BlockPos,
    nearest_player_distance_sqr: f64,
) -> bool {
    let ty = spawn_data.entity_type;
    let data = ty.data();
    if data.category == MobCategory::Misc {
        return false;
    }
    if !data.can_spawn_far_from_player
        && nearest_player_distance_sqr > despawn_distance_sqr(data.category)
    {
        return false;
    }
    if !data.summonable || !can_spawn_mob_at(level, category, spawn_data, pos) {
        return false;
    }
    if !spawn_placements::is_spawn_position_ok(ty, level, pos) {
        return false;
    }
    if !spawn_placements::check_spawn_rules_with_level_random(
        level,
        ty,
        EntitySpawnReason::Natural,
        pos,
    ) {
        return false;
    }
    ServerLevelAccessor::no_collision_aabb(
        level,
        &get_spawn_aabb(
            ty,
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
        ),
    )
}

/// `getMobForSpawn(level, type)`: `None` when the type cannot be created or is not a mob.
fn get_mob_for_spawn(
    level: &ServerLevel,
    ty: EntityType,
) -> Option<Box<dyn crate::world::entity::Entity>> {
    match ServerLevelAccessor::create_entity(
        level,
        ty,
        EntitySpawnRequest {
            reason: EntitySpawnReason::Natural,
            ignore_checks: false,
        },
    ) {
        Some(entity) if entity.is_mob() => Some(entity),
        Some(_) => {
            log::warn!("Can't spawn entity of type: {}", ty.name());
            None
        }
        None => None,
    }
}

/// `isValidPositionForMob(level, mob, nearestPlayerDistanceSqr)`.
fn is_valid_position_for_mob(
    level: &ServerLevel,
    mob: &dyn crate::world::entity::Entity,
    nearest_player_distance_sqr: f64,
) -> bool {
    if nearest_player_distance_sqr > despawn_distance_sqr(mob.get_type().data().category)
        && mob.remove_when_far_away(nearest_player_distance_sqr)
    {
        return false;
    }
    mob.check_spawn_rules(level, EntitySpawnReason::Natural) && mob.check_spawn_obstruction(level)
}

/// The tag `REDUCED_WATER_AMBIENT_SPAWNS` of the vanilla data pack, as biome keys.
fn reduced_water_ambient_biomes() -> &'static std::collections::HashSet<String> {
    static BIOMES: LazyLock<std::collections::HashSet<String>> = LazyLock::new(|| {
        mc_data::tags::names("worldgen/biome", "minecraft:reduced_water_ambient_spawns")
            .unwrap_or_default()
            .into_iter()
            .collect()
    });
    &BIOMES
}

/// `getRandomSpawnMobAt(level, structureManager, generator, category, random, pos)`.
fn get_random_spawn_mob_at(
    level: &mut ServerLevel,
    category: MobCategory,
    pos: BlockPos,
) -> Option<SpawnerData> {
    if category == MobCategory::WaterAmbient {
        let reduced = level
            .get_biome_id(pos)
            .zip(level.tick_state.biomes.as_ref())
            .is_some_and(|(id, registry)| {
                reduced_water_ambient_biomes().contains(&registry.key(id).to_string())
            });
        if reduced && level.tick_state.random.next_float() < 0.98f32 {
            return None;
        }
    }
    let mobs = mobs_at(level, category, pos);
    mobs.list()
        .get_random(&mut level.tick_state.random)
        .cloned()
}

/// `canSpawnMobAt(...)`.
fn can_spawn_mob_at(
    level: &ServerLevel,
    category: MobCategory,
    spawner_data: &SpawnerData,
    pos: BlockPos,
) -> bool {
    list_contains(mobs_at(level, category, pos).list(), spawner_data)
}

/// The list `mobsAt(...)` returns.
pub enum MobsAt {
    /// `NetherFortressStructure.FORTRESS_ENEMIES`.
    Fortress,
    /// A structure's `spawn_overrides` entry.
    StructureOverride(WeightedList<SpawnerData>),
    /// The biome spawns of the `natural_mob_spawns` attribute.
    Natural(Arc<MobSpawnSettings>, MobCategory),
}

impl MobsAt {
    pub fn list(&self) -> &WeightedList<SpawnerData> {
        match self {
            MobsAt::Fortress => MobSpawnSettings::fortress_enemies(),
            MobsAt::StructureOverride(list) => list,
            MobsAt::Natural(settings, category) => settings.get_mobs_to_spawn(*category),
        }
    }
}

/// A level's loaded chunks as structure data source (`getChunk(x, z, STRUCTURE_REFERENCES)`).
pub struct LevelStructureSource<'a>(pub &'a ServerLevel);

impl StructureDataSource for LevelStructureSource<'_> {
    fn structure_data(&self, pos: ChunkPos) -> Option<&mc_world::chunk::StructureData> {
        self.0
            .chunk_now(pos.x, pos.z)
            .map(|chunk| &chunk.data.access.structures)
    }
}

/// `mobsAt(level, structureManager, generator, category, pos)`.
pub fn mobs_at(level: &ServerLevel, category: MobCategory, pos: BlockPos) -> MobsAt {
    if is_in_nether_fortress_bounds(pos, level, category) {
        return MobsAt::Fortress;
    }
    // `ChunkGenerator.getMobsAt`
    let hooks = &level.chunk_source.chunk_map.gen_context.hooks;
    if let Some(list) = hooks.structure_spawn_override(&LevelStructureSource(level), pos, category)
    {
        return MobsAt::StructureOverride(list);
    }
    MobsAt::Natural(level.natural_mob_spawns_at(pos), category)
}

/// `isInNetherFortressBounds(pos, level, category, structureManager)`.
pub fn is_in_nether_fortress_bounds(
    pos: BlockPos,
    level: &ServerLevel,
    category: MobCategory,
) -> bool {
    if category == MobCategory::Monster
        && BlockGetter::get_block_state(level, pos.below()).block() == Block::NETHER_BRICKS
    {
        return level
            .chunk_source
            .chunk_map
            .gen_context
            .hooks
            .is_in_nether_fortress(&LevelStructureSource(level), pos);
    }
    false
}

/// `getRandomPosWithin(level, chunk)`.
fn get_random_pos_within(level: &mut ServerLevel, chunk_pos: ChunkPos) -> BlockPos {
    let x = chunk_pos.get_min_block_x() + level.tick_state.random.next_int_bounded(16);
    let z = chunk_pos.get_min_block_z() + level.tick_state.random.next_int_bounded(16);
    let top_empty_y = level.get_height_at(HeightmapType::WorldSurface, x, z) + 1;
    let min_y = level.get_min_y();
    let y = mth::random_between_inclusive(&mut level.tick_state.random, min_y, top_empty_y);
    BlockPos::new(x, y, z)
}

/// `isValidEmptySpawnBlock(level, pos, blockState, fluidState, type)`.
pub fn is_valid_empty_spawn_block(
    level: &dyn BlockGetter,
    pos: BlockPos,
    block_state: mc_data::block::BlockState,
    fluid_state: FluidState,
    ty: EntityType,
) -> bool {
    if block_state.is_collision_shape_full_block_at(level, pos)
        || block_state.is_signal_source()
        || !fluid_state.is_empty()
        || block_state.is_tag("prevent_mob_spawning_inside")
    {
        false
    } else {
        !dismount_helper::is_block_dangerous(ty, block_state)
    }
}

/// `spawnMobsForChunkGeneration(level, sourcePos, chunkPos, random)`.
pub fn spawn_mobs_for_chunk_generation(
    level: &mut dyn ServerLevelAccessor,
    source_pos: BlockPos,
    chunk_pos: ChunkPos,
    random: &mut dyn RandomSource,
) {
    if !level.spawn_mobs_rule() {
        return;
    }
    let mob_settings = level.natural_mob_spawns_at(source_pos);
    let mobs = mob_settings.get_mobs_to_spawn(MobCategory::Creature);
    if mobs.is_empty() {
        return;
    }
    let creature_probability = level.creature_world_gen_spawn_probability_at(source_pos);
    let xo = chunk_pos.get_min_block_x();
    let zo = chunk_pos.get_min_block_z();
    while random.next_float() < creature_probability {
        let Some(spawner_data) = mobs.get_random(random) else {
            continue;
        };
        let ty = spawner_data.entity_type;
        let count = spawner_data.count.sample(random);
        let mut group_spawn_data: Option<Box<dyn SpawnGroupData>> = None;
        let mut x = xo + random.next_int_bounded(16);
        let mut z = zo + random.next_int_bounded(16);
        let start_x = x;
        let start_z = z;
        for _ in 0..count {
            let mut success = false;
            let mut attempts = 0;
            while !success && attempts < 4 {
                let pos = get_top_non_colliding_pos(level, ty, x, z);
                if ty.data().summonable && spawn_placements::is_spawn_position_ok(ty, level, pos) {
                    let width = ty.data().width;
                    let fx = mth::clamp_f64(
                        f64::from(x),
                        f64::from(xo) + f64::from(width),
                        f64::from(xo) + 16.0 - f64::from(width),
                    );
                    let fz = mth::clamp_f64(
                        f64::from(z),
                        f64::from(zo) + f64::from(width),
                        f64::from(zo) + 16.0 - f64::from(width),
                    );
                    if !level.no_collision_aabb(&get_spawn_aabb(ty, fx, f64::from(pos.y()), fz))
                        || !spawn_placements::check_spawn_rules_with_level_random(
                            level,
                            ty,
                            EntitySpawnReason::ChunkGeneration,
                            BlockPos::containing(fx, f64::from(pos.y()), fz),
                        )
                    {
                        // `continue` of the `for (attempts...; attempts++)` loop skips the position update below
                        attempts += 1;
                        continue;
                    }
                    let Some(mut entity) = level.create_entity(
                        ty,
                        EntitySpawnRequest {
                            reason: EntitySpawnReason::Natural,
                            ignore_checks: false,
                        },
                    ) else {
                        attempts += 1;
                        continue;
                    };
                    let y_rot = random.next_float() * 360.0f32;
                    entity.snap_to(fx, f64::from(pos.y()), fz, y_rot, 0.0);
                    if entity.is_mob()
                        && entity.check_spawn_rules(&*level, EntitySpawnReason::ChunkGeneration)
                        && entity.check_spawn_obstruction(&*level)
                    {
                        let difficulty = level.get_current_difficulty_at(entity.block_position());
                        group_spawn_data = entity.finalize_spawn(
                            level,
                            difficulty,
                            EntitySpawnReason::ChunkGeneration,
                            group_spawn_data,
                        );
                        level.add_fresh_entity_with_passengers(entity);
                        success = true;
                    }
                }
                x += random.next_int_bounded(5) - random.next_int_bounded(5);
                z += random.next_int_bounded(5) - random.next_int_bounded(5);
                while x < xo || x >= xo + 16 || z < zo || z >= zo + 16 {
                    x = start_x + random.next_int_bounded(5) - random.next_int_bounded(5);
                    z = start_z + random.next_int_bounded(5) - random.next_int_bounded(5);
                }
                attempts += 1;
            }
        }
    }
}

/// `getTopNonCollidingPos(level, type, x, z)`.
fn get_top_non_colliding_pos(
    level: &dyn ServerLevelAccessor,
    ty: EntityType,
    x: i32,
    z: i32,
) -> BlockPos {
    let level_height = level.get_height_at(spawn_placements::get_heightmap_type(ty), x, z);
    let mut pos = BlockPos::new(x, level_height, z);
    if level.has_ceiling() {
        loop {
            pos = pos.below();
            if BlockGetter::get_block_state(level, pos).is_air() {
                break;
            }
        }
        loop {
            pos = pos.below();
            if !(BlockGetter::get_block_state(level, pos).is_air() && pos.y() > level.get_min_y()) {
                break;
            }
        }
    }
    spawn_placements::get_placement_type(ty).adjust_spawn_position(level, pos)
}
