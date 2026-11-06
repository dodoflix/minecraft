//! `ChunkGenerator.spawnOriginalMobs(region)` for noise generators: `NaturalSpawner.spawnMobsForChunkGeneration` over the
//! generation region with the region's decoration-seeded random.

use super::natural_spawner;
use crate::server::level::world_gen::{WorldGenContext, WorldGenRegion};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::gen_level::GenLevel;
use crate::world::level::levelgen::hooks::OriginalMobSpawner;
use mc_core::random::{LegacyRandomSource, WorldgenRandom, generate_unique_seed};
use std::sync::Arc;

/// The `OriginalMobSpawner` of a level (`VanillaWorldGenHooks::with_mob_spawner`).
pub struct NaturalOriginalMobs;

impl OriginalMobSpawner for NaturalOriginalMobs {
    fn spawn_original_mobs(
        &self,
        env: &Arc<GenEnv>,
        ctx: &WorldGenContext,
        region: &mut WorldGenRegion,
    ) {
        // `NoiseBasedChunkGenerator`: `!settings.disableMobGeneration()`; flat and debug generators never spawn
        if !env.generator.spawns_original_mobs() {
            return;
        }
        let center = region.get_center();
        let max_y = region.level_height.min_y + region.level_height.height - 1;
        let source_pos = center.get_world_position().at_y(max_y);
        let mut random =
            WorldgenRandom::new(Box::new(LegacyRandomSource::new(generate_unique_seed())));
        random.set_decoration_seed(ctx.seed, center.get_min_block_x(), center.get_min_block_z());
        let mut level = GenLevel::new(region, env.clone()).with_live_state(ctx.live.clone());
        natural_spawner::spawn_mobs_for_chunk_generation(
            &mut level,
            source_pos,
            center,
            &mut random,
        );
    }
}
