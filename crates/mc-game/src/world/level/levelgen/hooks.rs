//! `VanillaWorldGenHooks`: the [`WorldGenHooks`] of one level. Decoration builds the [`GenLevel`] over the lent region
//! and runs [`decoration::apply_biome_decoration`]; the structure stages are `structure::generation`; original mob
//! spawns are reached through the [`OriginalMobSpawner`] seam which the entity layer replaces with its implementation
//! (`VanillaWorldGenHooks::with_mob_spawner`).

use super::decoration::{self, DecorationInput, StepFeatureData};
use super::env::GenEnv;
use super::gen_level::GenLevel;
use super::structure::generation;
use crate::server::level::world_gen::{WorldGenContext, WorldGenHooks, WorldGenRegion};
use mc_world::chunk::ProtoChunk;
use std::sync::{Arc, OnceLock};

/// `ChunkGenerator.spawnOriginalMobs(region)` (`NaturalSpawner.spawnMobsForChunkGeneration`).
pub trait OriginalMobSpawner: Send + Sync {
    fn spawn_original_mobs(&self, env: &GenEnv, ctx: &WorldGenContext, region: &mut WorldGenRegion);
}

/// No original mob spawns.
pub struct NoOriginalMobs;

impl OriginalMobSpawner for NoOriginalMobs {
    fn spawn_original_mobs(&self, _env: &GenEnv, _ctx: &WorldGenContext, _region: &mut WorldGenRegion) {}
}

pub struct VanillaWorldGenHooks {
    env: Arc<GenEnv>,
    /// `ChunkGenerator.featuresPerStep` (memoized on first decoration like vanilla's `Suppliers.memoize`).
    features_per_step: OnceLock<Vec<StepFeatureData>>,
    mobs: Arc<dyn OriginalMobSpawner>,
}

impl VanillaWorldGenHooks {
    pub fn new(env: Arc<GenEnv>) -> Self {
        Self { env, features_per_step: OnceLock::new(), mobs: Arc::new(NoOriginalMobs) }
    }

    pub fn with_mob_spawner(mut self, mobs: Arc<dyn OriginalMobSpawner>) -> Self {
        self.mobs = mobs;
        self
    }

    pub fn env(&self) -> &Arc<GenEnv> {
        &self.env
    }

    /// `featuresPerStep.get()`; a feature order cycle is vanilla's `IllegalStateException` (uncaught: crashes the generation).
    pub fn features_per_step(&self) -> &[StepFeatureData] {
        self.features_per_step.get_or_init(|| StepFeatureData::build(&self.env).unwrap_or_else(|message| panic!("{message}")))
    }
}

impl WorldGenHooks for VanillaWorldGenHooks {
    fn create_structures(&self, _ctx: &WorldGenContext, chunk: &mut ProtoChunk) {
        generation::create_structures(&self.env, chunk);
    }

    fn create_references(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        generation::create_references(&self.env, region);
    }

    fn apply_biome_decoration(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        let features = self.features_per_step();
        let input = DecorationInput::from_region(region);
        let mut level = GenLevel::new(region, self.env.clone());
        decoration::apply_biome_decoration(&mut level, &self.env, features, &input);
    }

    fn spawn_original_mobs(&self, ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.mobs.spawn_original_mobs(&self.env, ctx, region);
    }

    fn on_structure_starts_available(&self, chunk: &ProtoChunk) {
        self.env.structure_check.on_structure_load_raw(chunk.access.pos, &chunk.access.structures);
    }
}
