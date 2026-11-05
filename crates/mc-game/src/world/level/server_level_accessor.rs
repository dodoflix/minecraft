//! `net.minecraft.world.level.ServerLevelAccessor`: what mob spawning (spawn predicates, `Mob.checkSpawnRules`,
//! `Mob.finalizeSpawn`, `NaturalSpawner.spawnMobsForChunkGeneration`) needs from "a level that can spawn entities",
//! shared by [`ServerLevel`] and the generation region ([`GenLevel`](super::levelgen::gen_level::GenLevel); in vanilla
//! `WorldGenRegion implements ServerLevelAccessor` and reaches the real level through `getLevel()`, which a worker thread
//! cannot do here, so the few `getLevel()` reads spawn code needs are methods of this trait).
//!
//! Dyn-compatible: spawn predicates and `Entity::check_spawn_rules` / `Entity::finalize_spawn` take
//! `&dyn ServerLevelAccessor` / `&mut dyn ServerLevelAccessor`. Add methods here (own marked section) when a mob needs
//! another read of the level.

use crate::server::level::ServerLevel;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::Entity;
use crate::world::entity::factory::EntitySpawnRequest;
use crate::world::level::LevelReader;
use mc_data::EntityType;
use crate::world::level::LevelAccessor;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_world::level_storage::Difficulty;
use super::spawner::mob_spawn_settings::MobSpawnSettings;
use std::sync::Arc;

pub trait ServerLevelAccessor: LevelAccessor + crate::world::entity::variant::SpawnContextLevel {
    /// `getDifficulty()`.
    fn get_difficulty(&self) -> Difficulty;
    /// `getCurrentDifficultyAt(pos)`.
    fn get_current_difficulty_at(&self, pos: BlockPos) -> DifficultyInstance;
    /// `getLevel().isThundering()`.
    fn is_thundering(&self) -> bool;
    /// `getLevel().isRaining()`.
    fn is_raining(&self) -> bool;
    /// `dimensionType().hasCeiling()`.
    fn has_ceiling(&self) -> bool;
    /// `dimensionType().monsterSpawnLightTest().sample(random)`.
    fn monster_spawn_light_test(&self, random: &mut dyn RandomSource) -> i32;
    /// `dimensionType().monsterSpawnBlockLightLimit()`.
    fn monster_spawn_block_light_limit(&self) -> i32;
    /// `addFreshEntityWithPassengers(entity)` for an entity without passengers (for a vehicle with passengers use
    /// `ServerLevel::add_fresh_entity_with_passengers`/the tree form of the implementor); false if it was rejected.
    fn add_fresh_entity_with_passengers(&mut self, entity: Box<dyn Entity>) -> bool;
    /// `noCollision(aabb)` (`noCollision(null, aabb)`).
    fn no_collision_aabb(&self, aabb: &AABB) -> bool;
    /// `isUnobstructed(entity)`: no building-blocking entity intersects the entity's bounding box.
    fn is_unobstructed_entity(&self, entity: &dyn Entity) -> bool;
    /// `getWorldBorder().isWithinBounds(pos)`.
    fn is_within_world_border(&self, pos: BlockPos) -> bool;
    /// `containsAnyLiquid(aabb)`.
    fn contains_any_liquid(&self, aabb: &AABB) -> bool;
    /// `getLevel().getGameRules().get(GameRules.SPAWN_MOBS)`.
    fn spawn_mobs_rule(&self) -> bool;
    /// `type.create(getLevel(), request)`: `None` when the factory yields nothing or the spawn checks fail
    /// (`EntityType.canSpawn(level)`: enabled features, peaceful difficulty).
    fn create_entity(&self, ty: EntityType, request: EntitySpawnRequest) -> Option<Box<dyn Entity>>;
    /// Runs `f` with this level's own random (`getRandom()`) taken out of the level, for calls vanilla makes as
    /// `f(this, ..., this.getRandom())` (the borrow checker cannot lend the level and its random at once; reads of the
    /// level random inside `f` through the level are not supported).
    fn with_random_taken(&mut self, f: &mut dyn FnMut(&dyn ServerLevelAccessor, &mut dyn RandomSource) -> bool) -> bool;
    /// `registryAccess()`.
    fn registry_access(&self) -> &crate::registry::RegistryAccess;
    /// `getBiome(pos)` as the registry id of the (block-resolution) biome; `None` while a needed chunk is missing.
    fn get_biome_id(&self, pos: BlockPos) -> Option<mc_world::palette::BiomeId>;
    /// `getLevel() instanceof ServerLevel` in the "real level" sense: the live server level, `None` while generating.
    fn as_server_level(&self) -> Option<&ServerLevel> {
        None
    }
    /// `environmentAttributes().getValue(NATURAL_MOB_SPAWNS, pos)`.
    fn natural_mob_spawns_at(&self, pos: BlockPos) -> Arc<MobSpawnSettings>;
    /// `environmentAttributes().getValue(CREATURE_WORLD_GEN_SPAWN_PROBABILITY, pos)`.
    fn creature_world_gen_spawn_probability_at(&self, pos: BlockPos) -> f32;
    // ==== [MobSpine] additions (ServerLevelAccessor methods) ====
    // ==== [Monsters2] additions (ServerLevelAccessor methods) ====
    /// `getNearestPlayer(x, y, z, distance, true)` (`EntitySelector.NO_CREATIVE_OR_SPECTATOR`); a generation region has no players.
    fn get_nearest_player_ignoring_creative(&self, x: f64, y: f64, z: f64, distance: f64) -> Option<crate::world::entity::EntityId> {
        self.as_server_level().and_then(|level| level.get_nearest_player(x, y, z, distance, crate::world::entity::mob::ext::no_creative_or_spectator))
    }
    // ==== [Cubes] additions (ServerLevelAccessor methods) ====
    /// `environmentAttributes().getValue(SURFACE_SLIME_SPAWN_CHANCE, pos)`.
    fn surface_slime_spawn_chance_at(&self, pos: BlockPos) -> f32 {
        self.as_server_level().map_or(0.0, |level| level.attribute_float(crate::world::attribute::attributes::SURFACE_SLIME_SPAWN_CHANCE, pos))
    }
    /// `level instanceof WorldGenLevel worldGenLevel` -> `worldGenLevel.getSeed()`.
    fn world_gen_seed(&self) -> Option<i64> {
        self.as_server_level().map(crate::world::level::levelgen::WorldGenLevel::get_seed)
    }
    // ==== [Undead] additions (ServerLevelAccessor methods) ====
    /// `getBiome(pos).is(tag)` for a biome tag (`BiomeTags.*`, the tag's path under `tags/worldgen/biome`).
    fn biome_is_tag(&self, pos: BlockPos, tag: &str) -> bool {
        let Some(id) = self.get_biome_id(pos) else { return false };
        let Ok(tag) = mc_core::ident::Identifier::parse(tag) else { return false };
        self.registry_access().registry("worldgen/biome").is_some_and(|r| r.is_in_tag(id.0, &tag))
    }
}

/// `IntProvider.sample(random)` of a data-loaded provider (`dimensionType().monsterSpawnLightTest()`): the modelled
/// kinds directly, any other through the worldgen provider decoder.
pub fn sample_int_provider(provider: &crate::registry::typed::int_provider::IntProvider, random: &mut dyn RandomSource) -> i32 {
    use crate::registry::typed::int_provider::IntProvider;
    match provider {
        IntProvider::Other { json, min_inclusive, .. } => match mc_worldgen::provider::IntProvider::from_json(json) {
            Ok(decoded) => decoded.sample(random),
            Err(_) => *min_inclusive,
        },
        modelled => modelled.sample(random).unwrap_or(0),
    }
}
