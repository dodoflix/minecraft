//! `VanillaWorldGenHooks`: the [`WorldGenHooks`] of one level. Decoration builds the [`GenLevel`] over the lent region
//! and runs [`decoration::apply_biome_decoration`]; the structure stages are `structure::generation`; original mob
//! spawns are reached through the [`OriginalMobSpawner`] hook, which the entity layer sets to its implementation
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
    fn spawn_original_mobs(
        &self,
        env: &Arc<GenEnv>,
        ctx: &WorldGenContext,
        region: &mut WorldGenRegion,
    );
}

/// No original mob spawns.
pub struct NoOriginalMobs;

impl OriginalMobSpawner for NoOriginalMobs {
    fn spawn_original_mobs(
        &self,
        _env: &Arc<GenEnv>,
        _ctx: &WorldGenContext,
        _region: &mut WorldGenRegion,
    ) {
    }
}

pub struct VanillaWorldGenHooks {
    env: Arc<GenEnv>,
    /// `ChunkGenerator.featuresPerStep` (memoized on first decoration like vanilla's `Suppliers.memoize`).
    features_per_step: OnceLock<Vec<StepFeatureData>>,
    mobs: Arc<dyn OriginalMobSpawner>,
}

impl VanillaWorldGenHooks {
    pub fn new(env: Arc<GenEnv>) -> Self {
        Self {
            env,
            features_per_step: OnceLock::new(),
            mobs: Arc::new(NoOriginalMobs),
        }
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
        self.features_per_step.get_or_init(|| {
            StepFeatureData::build(&self.env).unwrap_or_else(|message| panic!("{message}"))
        })
    }
}

impl WorldGenHooks for VanillaWorldGenHooks {
    fn gen_env(&self) -> Option<&Arc<GenEnv>> {
        Some(&self.env)
    }

    fn create_structures(&self, _ctx: &WorldGenContext, chunk: &mut ProtoChunk) {
        generation::create_structures(&self.env, chunk);
    }

    fn create_references(&self, _ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        generation::create_references(&self.env, region);
    }

    fn apply_biome_decoration(&self, ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        let features = self.features_per_step();
        let input = DecorationInput::from_region(region);
        let mut level = GenLevel::new(region, self.env.clone()).with_live_state(ctx.live.clone());
        decoration::apply_biome_decoration(&mut level, &self.env, features, &input);
    }

    fn spawn_original_mobs(&self, ctx: &WorldGenContext, region: &mut WorldGenRegion) {
        self.mobs.spawn_original_mobs(&self.env, ctx, region);
    }

    fn structure_spawn_override(
        &self,
        source: &dyn super::structure::manager::StructureDataSource,
        pos: mc_core::pos::BlockPos,
        category: mc_data::enums::MobCategory,
    ) -> Option<mc_worldgen::provider::WeightedList<super::structure::structure::SpawnerData>> {
        use super::structure::manager::StructureManager;
        use super::structure::structure::BoundingBoxType;
        let manager = StructureManager::new(&self.env);
        // `getAllStructuresAt(pos)`: the structures referenced by the chunk of `pos`
        let all = manager.get_all_structures_at(source, pos)?;
        for (id, references) in all {
            let Some(identifier) = mc_core::ident::Identifier::try_parse(id) else {
                continue;
            };
            let Some(structure) = self.env.structures.get(&identifier) else {
                continue;
            };
            let Some(spawn_override) = structure.value().settings().spawn_override(category) else {
                continue;
            };
            let mut in_override_box = false;
            manager.fill_starts_for_structure(source, structure, references, &mut |start| {
                if !in_override_box {
                    let mut start = start.lock();
                    in_override_box = match spawn_override.bounding_box {
                        BoundingBoxType::Piece => manager.structure_has_piece_at(pos, &start),
                        BoundingBoxType::Structure => start.get_bounding_box().is_inside(&pos),
                    };
                }
            });
            if in_override_box {
                return Some(spawn_override.spawns.clone());
            }
        }
        None
    }

    fn is_in_nether_fortress(
        &self,
        source: &dyn super::structure::manager::StructureDataSource,
        pos: mc_core::pos::BlockPos,
    ) -> bool {
        let Some(fortress) = mc_core::ident::Identifier::try_parse("minecraft:fortress")
            .and_then(|id| self.env.structures.get(&id).cloned())
        else {
            return false;
        };
        super::structure::manager::StructureManager::new(&self.env)
            .get_structure_at(source, pos, &fortress)
            .is_some()
    }

    fn on_structure_starts_available(&self, chunk: &ProtoChunk) {
        self.env
            .structure_check
            .on_structure_load_raw(chunk.access.pos, &chunk.access.structures);
    }

    fn has_structure_piece_at(
        &self,
        source: &dyn super::structure::manager::StructureDataSource,
        pos: mc_core::pos::BlockPos,
        matcher: &dyn Fn(&mc_core::ident::Identifier) -> bool,
    ) -> bool {
        super::structure::manager::StructureManager::new(&self.env)
            .get_structure_with_piece_at(source, pos, matcher)
            .is_some()
    }
}
