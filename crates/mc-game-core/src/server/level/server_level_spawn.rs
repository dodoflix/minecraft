//! `ServerLevel` as a [`ServerLevelAccessor`] (spawn predicates, `finalizeSpawn`, natural spawning) and the per-tick
//! publication of the values chunk generation reads from the live level.

use super::ServerLevel;
use super::level_tick::LevelTickEnv;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::Entity;
use crate::world::entity::factory::{self, EntitySpawnRequest};
use crate::world::level::collision::CollisionGetter as _;
use crate::world::level::server_level_accessor::ServerLevelAccessor;
use crate::world::level::spawner::mob_spawn_settings::MobSpawnSettings;
use mc_core::phys::AABB;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::EntityType;
use mc_world::level_storage::Difficulty;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

impl ServerLevel {
    /// Publishes the live values worldgen threads and `getCurrentDifficultyAt` read (once per tick, before the chunk source
    /// ticks).
    pub fn publish_spawn_environment(&mut self, env: &dyn LevelTickEnv) {
        self.spawning.overworld_clock_time = env.overworld_clock_time();
        self.spawning.moon_brightness = self.get_moon_brightness(BlockPos::ZERO);
        let spawn_mobs = self.spawn_mobs();
        self.spawning.gen_live.publish(
            self.difficulty(),
            spawn_mobs,
            self.spawning.overworld_clock_time,
            self.spawning.moon_brightness,
        );
    }
}

impl ServerLevelAccessor for ServerLevel {
    fn get_difficulty(&self) -> Difficulty {
        self.difficulty()
    }

    fn get_current_difficulty_at(&self, pos: BlockPos) -> DifficultyInstance {
        let (mut local_time, mut moon_brightness) = (0i64, 0.0f32);
        if let Some(chunk) = self.chunk_now(pos.x() >> 4, pos.z() >> 4) {
            local_time = chunk.data.access.inhabited_time();
            moon_brightness = self.spawning.moon_brightness;
        }
        DifficultyInstance::new(
            self.difficulty(),
            self.spawning.overworld_clock_time,
            local_time,
            moon_brightness,
        )
    }

    fn is_thundering(&self) -> bool {
        ServerLevel::is_thundering(self)
    }

    fn is_raining(&self) -> bool {
        ServerLevel::is_raining(self)
    }

    fn has_ceiling(&self) -> bool {
        self.has_ceiling
    }

    fn monster_spawn_light_test(&self, random: &mut dyn RandomSource) -> i32 {
        match &self.spawning.dimension_type {
            Some(dimension_type) => {
                crate::world::level::server_level_accessor::sample_int_provider(
                    dimension_type.monster_spawn_light_test(),
                    random,
                )
            }
            // the overworld's `uniform(0, 7)` for levels without a dimension type (detached test levels)
            None => IntProvider::Uniform {
                min_inclusive: 0,
                max_inclusive: 7,
            }
            .sample(random),
        }
    }

    fn monster_spawn_block_light_limit(&self) -> i32 {
        self.spawning
            .dimension_type
            .as_ref()
            .map_or(0, |d| d.monster_spawn_block_light_limit())
    }

    fn add_fresh_entity_with_passengers(&mut self, entity: Box<dyn Entity>) -> bool {
        self.add_fresh_entity(entity)
    }

    fn no_collision_aabb(&self, aabb: &AABB) -> bool {
        self.no_collision(None, None, aabb)
    }

    fn is_unobstructed_entity(&self, entity: &dyn Entity) -> bool {
        self.is_unobstructed(
            Some(entity.id()),
            &mc_core::phys::shapes::Shapes::create(entity.get_bounding_box()),
        )
    }

    fn is_within_world_border(&self, pos: BlockPos) -> bool {
        ServerLevel::is_within_world_border(self, pos)
    }

    fn contains_any_liquid(&self, aabb: &AABB) -> bool {
        crate::world::entity::movement::contains_any_liquid(self, aabb)
    }

    fn as_server_level(&self) -> Option<&ServerLevel> {
        Some(self)
    }

    fn registry_access(&self) -> &crate::registry::RegistryAccess {
        &self.registries
    }

    fn get_biome_id(&self, pos: BlockPos) -> Option<mc_world::palette::BiomeId> {
        ServerLevel::get_biome_id(self, pos)
    }

    fn spawn_mobs_rule(&self) -> bool {
        self.spawn_mobs()
    }

    fn create_entity(
        &self,
        ty: EntityType,
        request: EntitySpawnRequest,
    ) -> Option<Box<dyn Entity>> {
        factory::create(ty, self, request)
    }

    fn with_random_taken(
        &mut self,
        f: &mut dyn FnMut(&dyn ServerLevelAccessor, &mut dyn RandomSource) -> bool,
    ) -> bool {
        // `Level.random` is a shared handle: draws through the clone are draws of the level's random
        let mut random = self.tick_state.random.clone();
        f(&*self, &mut random)
    }

    fn natural_mob_spawns_at(&self, pos: BlockPos) -> Arc<MobSpawnSettings> {
        let biome = self.get_biome_id(pos);
        match self.spawning.biome_table(self.tick_state.biomes.as_ref()) {
            Some(table) => table.natural_mob_spawns(&self.spawning.dimension_base, biome),
            None => self.spawning.dimension_base.natural_mob_spawns.clone(),
        }
    }

    fn creature_world_gen_spawn_probability_at(&self, pos: BlockPos) -> f32 {
        let biome = self.get_biome_id(pos);
        match self.spawning.biome_table(self.tick_state.biomes.as_ref()) {
            Some(table) => {
                table.creature_world_gen_spawn_probability(&self.spawning.dimension_base, biome)
            }
            None => self.spawning.dimension_base.creature_probability,
        }
    }
}
