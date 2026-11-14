//! `GenLevel`: vanilla's `WorldGenRegion` (the `WorldGenLevel` features, placements, structures and block behaviours run
//! against during chunk generation) over the chunks lent to one generation step ([`WorldGenRegion`] of
//! `server::level::world_gen`).
//!
//! Semantics follow `WorldGenRegion` exactly:
//! * reads (`getBlockState`, `getFluidState`, `getHeight`, ...) outside the step's block-state write zone log vanilla's
//!   `Detected unsafe terrain read during worldgen` error (and continue); a chunk the step may not see is the crash
//!   `Requested chunk unavailable during world generation` (a panic here);
//! * writes go through `ensureCanWrite` (`Detected setBlock in a far chunk` + dropped outside the write zone, the
//!   upgrading-chunk height rule), then `ProtoChunk.setBlockState` (heightmaps per the chunk's status, light engine calls queued in `WorldGenRegion::light_events`; POI
//!   type changes are queued in `WorldGenRegion::poi_changes` for the game thread), the `DUMMY` block entity NBT of block-entity states (the real NBT arrives through
//!   [`WorldGenLevel::set_block_entity_nbt`]) and `getPostProcessPos` post-processing marks (`flags & 16 == 0`);
//! * chunks the game thread merely lends read-only (`GenChunk::Snapshot`, vanilla `ImposterProtoChunk(allowWrites=false)`)
//!   drop block, block entity, post-processing, entity and tick writes silently;
//! * scheduled ticks land in the owning chunk's `ProtoChunkTicks` (delay is not stored), game events, level events,
//!   sounds, particles and neighbor updates are no-ops, entity queries are empty and there are no live block entities.
//!
//! `get_height_at` on a heightmap the chunk has not primed (vanilla `ChunkAccess.getHeight` primes and keeps it, so it is
//! stale for types the chunk's status does not maintain) answers from a temporary priming and records the request; the
//! chunk is primed before the next mutation, on [`GenLevel::region`] and on drop, which is the same chunk state.

use super::env::GenEnv;
use super::world_gen_level::WorldGenLevel;
use crate::server::level::world_gen::{GenChunk, LightEvents, WorldGenRegion};
use crate::world::block_entity::BlockEntity;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::ai::village::poi::PoiTypes;
use crate::world::level::block::BlockStateExt;
use crate::world::level::chunk::status::ChunkStep;
use crate::world::level::{BlockGetter, LevelAccessor, LevelHeight, LevelReader, LightLayer};
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_data::fluid::{Fluid, FluidState};
use mc_nbt::CompoundTag;
use mc_world::chunk::{ProtoChunk, StructureData};
use mc_world::heightmap::{Heightmap, HeightmapMap, HeightmapType};
use mc_world::level_storage::Difficulty;
use mc_world::palette::BiomeId;
use mc_world::status::ChunkStatus;
use mc_world::ticks::{ScheduledTick, TickAccess, TickPriority};
use mc_worldgen::biome::{BiomeManager, BiomeResolver};
use std::cell::RefCell;
use std::sync::Arc;

/// `WorldGenRegion` over the chunks of one generation step.
pub struct GenLevel<'a> {
    region: &'a mut WorldGenRegion,
    env: Arc<GenEnv>,
    /// `random`: `randomState.getOrCreateRandomFactory("worldgen_region_random").at(center.getWorldPosition())`.
    random: Box<dyn RandomSource>,
    /// `subTickCount`.
    sub_tick_count: i64,
    /// `currentlyGenerating`.
    currently_generating: Option<Box<dyn Fn() -> String + Send + Sync>>,
    /// `BiomeManager.obfuscateSeed(seed)`.
    biome_zoom_seed: i64,
    /// `generatingStep.targetStatus()`.
    target_status: ChunkStatus,
    /// `generatingStep.directDependencies()` as the maximum status per chunk distance (`None`: every chunk of the
    /// region's read radius is available with any status).
    max_allowed: Option<Vec<ChunkStatus>>,
    /// `levelData.getGameTime()`.
    game_time: i64,
    /// `level.getDifficulty()`, `level.getOverworldClockTime()` and `level.getMoonBrightness(pos)` (a flat value here).
    difficulty: (Difficulty, i64, f32),
    /// The live level values mob spawning reads (difficulty, `spawnMobs`, registries, dimension type).
    live: Option<Arc<crate::server::level::gen_live_state::GenLiveState>>,
    /// `getLightEngine()` of the live level (what the spawn step reads: `LIGHT` precedes `SPAWN`).
    light: Option<mc_light::LightReader>,
    /// Heightmaps read while missing from their (writable) chunk: `(chunkX, chunkZ, type)`, primed lazily.
    pending_heightmaps: RefCell<Vec<(i32, i32, HeightmapType)>>,
}

impl<'a> GenLevel<'a> {
    /// `new WorldGenRegion(level, cache, step, center)` for the feature step (`ChunkStatus.FEATURES`).
    pub fn new(region: &'a mut WorldGenRegion, env: Arc<GenEnv>) -> GenLevel<'a> {
        let random = env
            .random_state
            .get_or_create_random_factory(
                &Identifier::with_default_namespace("worldgen_region_random")
                    .expect("valid identifier"),
            )
            .at_pos(region.center.get_world_position());
        let biome_zoom_seed = BiomeManager::obfuscate_seed(env.seed);
        GenLevel {
            region,
            env,
            random,
            sub_tick_count: 0,
            currently_generating: None,
            biome_zoom_seed,
            target_status: ChunkStatus::Features,
            max_allowed: None,
            game_time: 0,
            difficulty: (Difficulty::Normal, 0, 0.0),
            live: None,
            light: None,
            pending_heightmaps: RefCell::new(Vec::new()),
        }
    }

    /// Binds the region to `step` (target status in messages, per-distance maximum statuses of `getChunk`).
    pub fn with_step(mut self, step: &ChunkStep) -> GenLevel<'a> {
        self.target_status = step.target_status;
        self.max_allowed = Some(step.direct_dependencies.as_list().to_vec());
        self
    }

    /// `levelData.getGameTime()` (only the trigger time of scheduled ticks depends on it).
    pub fn with_game_time(mut self, game_time: i64) -> GenLevel<'a> {
        self.game_time = game_time;
        self
    }

    /// The server difficulty, the overworld clock time and the moon brightness `getCurrentDifficultyAt` reports (default:
    /// normal difficulty, clock 0, moon 0).
    pub fn with_difficulty(
        mut self,
        difficulty: Difficulty,
        overworld_clock_time: i64,
        moon_brightness: f32,
    ) -> GenLevel<'a> {
        self.difficulty = (difficulty, overworld_clock_time, moon_brightness);
        self
    }

    /// Binds the region to the live level values (`getLevel()` reads: mob spawning, template entities); also sets the difficulty inputs.
    pub fn with_live_state(
        mut self,
        live: Arc<crate::server::level::gen_live_state::GenLiveState>,
    ) -> GenLevel<'a> {
        self.difficulty = (
            live.difficulty(),
            live.overworld_clock_time(),
            live.moon_brightness(),
        );
        self.live = Some(live);
        self
    }

    /// Binds the light engine of the live level (what the spawn step reads: `LIGHT` precedes `SPAWN`). The features step
    /// leaves it unbound: the engine reads full sky light for sections it has not lit yet where vanilla's decoration sees
    /// them dark (swamp mushrooms next to a hut), so there light reads stay 0.
    pub fn with_live_light(mut self) -> GenLevel<'a> {
        self.light = self.live.as_ref().and_then(|live| live.light().cloned());
        self
    }

    /// The lent chunks again (e.g. to read back results).
    pub fn region(&mut self) -> &WorldGenRegion {
        self.flush_heightmaps();
        self.region
    }

    /// Primes the heightmaps `get_height_at` found missing (`ChunkAccess.getHeight` keeps what it primes).
    fn flush_heightmaps(&mut self) {
        for (chunk_x, chunk_z, ty) in self.pending_heightmaps.get_mut().drain(..) {
            if self
                .region
                .gen_chunk(chunk_x, chunk_z)
                .is_some_and(GenChunk::is_writable)
            {
                let chunk = self
                    .region
                    .get_chunk_mut_unchecked(ChunkPos::new(chunk_x, chunk_z));
                if !chunk.access.has_primed_heightmap(ty) {
                    chunk.access.prime_heightmaps(true, &[ty]);
                }
            }
        }
    }

    fn max_allowed_status(&self, distance: i32) -> Option<ChunkStatus> {
        match &self.max_allowed {
            Some(list) => usize::try_from(distance)
                .ok()
                .and_then(|d| list.get(d))
                .copied(),
            None => (distance <= self.region.read_radius).then_some(ChunkStatus::Full),
        }
    }

    /// `getChunk(chunkX, chunkZ, targetStatus)`: the crash report of vanilla (a panic) when the step may not see the chunk.
    fn chunk_with_status(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        target_status: ChunkStatus,
    ) -> &ProtoChunk {
        let distance = self
            .region
            .center
            .get_chessboard_distance_xz(chunk_x, chunk_z);
        let max_allowed = self.max_allowed_status(distance);
        let chunk = if max_allowed.is_some() {
            self.region.get_chunk(chunk_x, chunk_z)
        } else {
            None
        };
        if let (Some(chunk), Some(max)) = (chunk, max_allowed)
            && target_status <= max
        {
            return chunk;
        }
        panic!(
            "Exception generating new chunk: Requested chunk unavailable during world generation\n\
             Chunk request details:\n\
             \tRequested chunk: {chunk_x}, {chunk_z}\n\
             \tGenerating status: {}\n\
             \tRequested status: {}\n\
             \tActual status: {}\n\
             \tMaximum allowed status: {}\n\
             \tRequested distance: {distance}\n\
             \tGenerating chunk: {}",
            self.target_status.name(),
            target_status.name(),
            chunk.map_or_else(
                || "[out of cache bounds]".to_string(),
                |c| c.get_persisted_status().name()
            ),
            max_allowed.map_or_else(|| "null".to_string(), |s| s.name()),
            self.region.center,
        );
    }

    /// `getChunk(chunkX, chunkZ)` (`ChunkStatus.EMPTY`).
    fn chunk(&self, chunk_x: i32, chunk_z: i32) -> &ProtoChunk {
        self.chunk_with_status(chunk_x, chunk_z, ChunkStatus::Empty)
    }

    /// The chunk of `pos` for a mutation: `None` when it is a read-only snapshot (`ImposterProtoChunk` drops the write).
    fn chunk_for_write(&mut self, pos: BlockPos) -> Option<&mut ProtoChunk> {
        let (chunk_x, chunk_z) = (pos.x() >> 4, pos.z() >> 4);
        self.chunk(chunk_x, chunk_z);
        if self
            .region
            .gen_chunk(chunk_x, chunk_z)
            .is_some_and(GenChunk::is_writable)
        {
            Some(
                self.region
                    .get_chunk_mut_unchecked(ChunkPos::new(chunk_x, chunk_z)),
            )
        } else {
            None
        }
    }

    fn is_within_write_zone_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        let center = self.region.center;
        (center.x - chunk_x).abs() <= self.region.write_radius
            && (center.z - chunk_z).abs() <= self.region.write_radius
    }

    fn currently_generating_suffix(&self) -> String {
        match &self.currently_generating {
            Some(what) => format!(", currently generating: {}", what()),
            None => String::new(),
        }
    }

    /// `warnIfReadOutsideWriteZone(chunkX, chunkZ)`.
    fn warn_if_read_outside_write_zone(&self, chunk_x: i32, chunk_z: i32) {
        let center = self.region.center;
        if (center.x != chunk_x || center.z != chunk_z)
            && !self.is_within_write_zone_chunk(chunk_x, chunk_z)
        {
            let read_distance = (center.x - chunk_x).abs().max((center.z - chunk_z).abs());
            log::error!(
                "Detected unsafe terrain read during worldgen: reading from chunk [{}, {}] while generating chunk [{}, {}] (distance: {}, write radius: {}), step: {}{}",
                chunk_x,
                chunk_z,
                center.x,
                center.z,
                read_distance,
                self.region.write_radius,
                self.target_status.name(),
                self.currently_generating_suffix()
            );
        }
    }

    /// `findBlocksIn(from, to)`'s read-zone check (the scan itself is the `LevelReader` default).
    pub fn warn_if_find_blocks_outside_write_zone(&self, from: BlockPos, to: BlockPos) {
        for chunk_z in (from.z() >> 4)..=(to.z() >> 4) {
            for chunk_x in (from.x() >> 4)..=(to.x() >> 4) {
                self.warn_if_read_outside_write_zone(chunk_x, chunk_z);
            }
        }
    }

    /// `ImposterProtoChunk.fixType`: read-only snapshots keep only the final heightmaps.
    fn fix_type(ty: HeightmapType) -> HeightmapType {
        match ty {
            HeightmapType::WorldSurfaceWg => HeightmapType::WorldSurface,
            HeightmapType::OceanFloorWg => HeightmapType::OceanFloor,
            other => other,
        }
    }

    /// `ScheduledTickAccess.createTick` + `getBlockTicks().schedule(..)` for either tick container.
    fn schedule_tick<T: Copy + Eq + std::hash::Hash>(
        &mut self,
        pos: BlockPos,
        ty: T,
        delay: i32,
        priority: TickPriority,
        container: impl FnOnce(&mut ProtoChunk) -> &mut dyn TickAccess<T>,
    ) {
        let sub_tick_order = self.next_sub_tick_count();
        let tick = ScheduledTick {
            ty,
            pos,
            trigger_tick: self.game_time.wrapping_add(i64::from(delay)),
            priority,
            sub_tick_order,
        };
        if let Some(chunk) = self.chunk_for_write(pos) {
            container(chunk).schedule(tick);
        }
    }
}

impl Drop for GenLevel<'_> {
    fn drop(&mut self) {
        self.flush_heightmaps();
    }
}

struct RegionBiomes<'r, 'a>(&'r GenLevel<'a>);

impl BiomeResolver for RegionBiomes<'_, '_> {
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self.0.get_noise_biome(quart_x, quart_y, quart_z)
    }
}

impl BlockGetter for GenLevel<'_> {
    fn get_block_state(&self, pos: BlockPos) -> BlockState {
        let (chunk_x, chunk_z) = (pos.x() >> 4, pos.z() >> 4);
        self.warn_if_read_outside_write_zone(chunk_x, chunk_z);
        self.chunk(chunk_x, chunk_z).get_block_state(pos)
    }

    fn get_fluid_state(&self, pos: BlockPos) -> FluidState {
        let (chunk_x, chunk_z) = (pos.x() >> 4, pos.z() >> 4);
        self.warn_if_read_outside_write_zone(chunk_x, chunk_z);
        self.chunk(chunk_x, chunk_z).get_fluid_state(pos)
    }

    /// Live block entities do not exist during generation (the chunk only holds raw NBT, see
    /// [`WorldGenLevel::get_block_entity_nbt`]).
    fn get_block_entity(&self, _pos: BlockPos) -> Option<&dyn BlockEntity> {
        None
    }

    fn level_height(&self) -> LevelHeight {
        self.region.level_height
    }
}

impl LevelReader for GenLevel<'_> {
    /// The dimension type's `fast_lava` (only the nether's has it; custom dimension types are not consulted here).
    fn is_fast_lava(&self) -> bool {
        self.env.dimension.to_string() == "minecraft:the_nether"
    }

    /// `hasChunk`: `distance < directDependencies.size()`.
    fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.max_allowed_status(
            self.region
                .center
                .get_chessboard_distance_xz(chunk_x, chunk_z),
        )
        .is_some()
    }

    /// `getHeight(type, x, z)`: the chunk's heightmap value `+ 1`.
    fn get_height_at(&self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        let (chunk_x, chunk_z) = (x >> 4, z >> 4);
        self.warn_if_read_outside_write_zone(chunk_x, chunk_z);
        let chunk = self.chunk(chunk_x, chunk_z);
        let ty = if self
            .region
            .gen_chunk(chunk_x, chunk_z)
            .is_some_and(GenChunk::is_writable)
        {
            ty
        } else {
            Self::fix_type(ty)
        };
        match chunk.access.heightmaps.get(&ty) {
            Some(heightmap) => heightmap.get_first_available(x & 15, z & 15),
            None => {
                if self
                    .region
                    .gen_chunk(chunk_x, chunk_z)
                    .is_some_and(GenChunk::is_writable)
                {
                    let mut pending = self.pending_heightmaps.borrow_mut();
                    if !pending.contains(&(chunk_x, chunk_z, ty)) {
                        pending.push((chunk_x, chunk_z, ty));
                    }
                }
                let mut primed = HeightmapMap::new();
                Heightmap::prime_heightmaps(&chunk.access.view(true), &mut primed, &[ty]);
                primed[&ty].get_first_available(x & 15, z & 15)
            }
        }
    }

    fn get_sea_level(&self) -> i32 {
        self.env.get_sea_level()
    }

    /// The light engine of the live level, when the region is bound to one (the spawn step); a bare region reads 0.
    fn get_brightness(&self, layer: LightLayer, pos: BlockPos) -> i32 {
        let layer = match layer {
            LightLayer::Sky => mc_light::LightLayer::Sky,
            LightLayer::Block => mc_light::LightLayer::Block,
        };
        self.light
            .as_ref()
            .map_or(0, |l| l.get_light_value(layer, pos))
    }

    fn get_sky_darken(&self) -> i32 {
        0
    }

    fn get_raw_brightness(&self, pos: BlockPos, sky_darkening: i32) -> i32 {
        self.light
            .as_ref()
            .map_or(0, |l| l.get_raw_brightness(pos, sky_darkening))
    }

    fn can_see_sky(&self, pos: BlockPos) -> bool {
        self.light
            .as_ref()
            .is_some_and(|l| l.get_light_value(mc_light::LightLayer::Sky, pos) >= 15)
    }
}

impl LevelAccessor for GenLevel<'_> {
    fn mark_for_post_processing(&mut self, pos: BlockPos) {
        WorldGenLevel::mark_pos_for_post_processing(self, pos);
    }
    fn set_block(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        flags: u32,
        _update_limit: i32,
    ) -> bool {
        if !self.ensure_can_write(pos) {
            return false;
        }
        self.flush_heightmaps();
        let mut old_state_written = None;
        let mut light = LightEvents::default();
        if let Some(chunk) = self.chunk_for_write(pos) {
            let old_state = chunk.set_block_state(pos, state, Some(&mut light));
            old_state_written = Some(old_state);
            if state.has_block_entity() {
                // A ProtoChunk never has the LEVELCHUNK type: the placeholder the real block entity NBT replaces. A live
                // block entity (anything but the placeholder: vanilla keeps the instance in `ProtoChunk.blockEntities`, the
                // placeholder only goes to the pending NBT) survives the block being set again.
                let live = chunk
                    .access
                    .get_block_entity_nbt(pos)
                    .is_some_and(|tag| tag.get_string("id") != Some("DUMMY"));
                if !live {
                    let mut tag = CompoundTag::new();
                    tag.put_int("x", pos.x());
                    tag.put_int("y", pos.y());
                    tag.put_int("z", pos.z());
                    tag.put_string("id", "DUMMY");
                    chunk.access.set_block_entity_nbt(tag);
                }
            } else if old_state.has_block_entity() {
                chunk.access.remove_block_entity(pos);
            }
        }
        self.region.light_events.append(&mut light.0);
        if let Some(old_state) = old_state_written {
            // `level.updatePOIOnBlockStateChange(pos, oldState, blockState)`: applied by the game thread with the batch
            let (old_type, new_type) = (PoiTypes::for_state(old_state), PoiTypes::for_state(state));
            if old_type != new_type {
                self.region.poi_changes.push((pos, old_type, new_type));
            }
        }
        if flags & 16 == 0
            && let Some(post_process_pos) = state.get_post_process_pos(&*self, pos)
        {
            self.mark_pos_for_post_processing(post_process_pos);
        }
        true
    }

    fn as_world_gen_level(&self) -> Option<&dyn WorldGenLevel> {
        Some(self)
    }

    fn remove_block(&mut self, pos: BlockPos, _moved_by_piston: bool) -> bool {
        self.set_block_and_update(pos, Block::AIR.default_state())
    }

    fn destroy_block(
        &mut self,
        pos: BlockPos,
        _drop_block: bool,
        _breaker: Option<i32>,
        update_limit: i32,
    ) -> bool {
        if self.get_block_state(pos).is_air() {
            false
        } else {
            self.set_block(pos, Block::AIR.default_state(), 3, update_limit)
        }
    }

    fn get_game_time(&self) -> i64 {
        self.game_time
    }

    fn next_sub_tick_count(&mut self) -> i64 {
        let count = self.sub_tick_count;
        self.sub_tick_count += 1;
        count
    }

    fn random(&mut self) -> &mut dyn RandomSource {
        &mut *self.random
    }

    fn schedule_block_tick(
        &mut self,
        pos: BlockPos,
        block: Block,
        delay: i32,
        priority: TickPriority,
    ) {
        self.schedule_tick(pos, block, delay, priority, |chunk| &mut chunk.block_ticks);
    }

    fn schedule_fluid_tick(
        &mut self,
        pos: BlockPos,
        fluid: Fluid,
        delay: i32,
        priority: TickPriority,
    ) {
        self.schedule_tick(pos, fluid, delay, priority, |chunk| &mut chunk.fluid_ticks);
    }

    fn has_scheduled_block_tick(&self, pos: BlockPos, block: Block) -> bool {
        self.region_chunk_ticks(pos, |chunk| {
            chunk.block_ticks.has_scheduled_tick(&pos, block)
        })
    }

    fn has_scheduled_fluid_tick(&self, pos: BlockPos, fluid: Fluid) -> bool {
        self.region_chunk_ticks(pos, |chunk| {
            chunk.fluid_ticks.has_scheduled_tick(&pos, fluid)
        })
    }

    /// No neighbor updates during generation.
    fn update_neighbors_at(&mut self, _pos: BlockPos, _source_block: Block) {}
}

impl GenLevel<'_> {
    /// `getChunk(pos).getBlockTicks()` lookups: read-only snapshots have the black hole (never scheduled).
    fn region_chunk_ticks(&self, pos: BlockPos, f: impl FnOnce(&ProtoChunk) -> bool) -> bool {
        let (chunk_x, chunk_z) = (pos.x() >> 4, pos.z() >> 4);
        let chunk = self.chunk(chunk_x, chunk_z);
        self.region
            .gen_chunk(chunk_x, chunk_z)
            .is_some_and(GenChunk::is_writable)
            && f(chunk)
    }
}

impl WorldGenLevel for GenLevel<'_> {
    fn get_seed(&self) -> i64 {
        self.env.seed
    }

    fn get_current_difficulty_at(&self, pos: BlockPos) -> DifficultyInstance {
        assert!(
            self.has_chunk(pos.x() >> 4, pos.z() >> 4),
            "We are asking a region for a chunk out of bound"
        );
        let (difficulty, clock_time, moon_brightness) = self.difficulty;
        DifficultyInstance::new(difficulty, clock_time, 0, moon_brightness)
    }

    fn env(&self) -> &Arc<GenEnv> {
        &self.env
    }

    fn ensure_can_write(&mut self, pos: BlockPos) -> bool {
        if !self.is_within_write_zone(pos) {
            log::error!(
                "Detected setBlock in a far chunk [{}, {}], pos: {}, status: {}{}",
                pos.x() >> 4,
                pos.z() >> 4,
                pos,
                self.target_status.name(),
                self.currently_generating_suffix()
            );
            return false;
        }
        let center = self.chunk(self.region.center.x, self.region.center.z);
        if center.is_upgrading() {
            return !center
                .get_height_accessor_for_generation()
                .is_outside_build_height(pos.y());
        }
        true
    }

    fn is_within_write_zone(&self, pos: BlockPos) -> bool {
        self.is_within_write_zone_chunk(pos.x() >> 4, pos.z() >> 4)
    }

    fn set_currently_generating(&mut self, what: Option<Box<dyn Fn() -> String + Send + Sync>>) {
        self.currently_generating = what;
    }

    /// `getBiomeManager().getBiome(pos)`.
    fn get_biome(&self, pos: BlockPos) -> BiomeId {
        BiomeManager::new(&RegionBiomes(self), self.biome_zoom_seed).get_biome(pos)
    }

    /// `LevelReader.getNoiseBiome`: the biome stored in the chunk (`getChunk(.., BIOMES, false)`).
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        let (chunk_x, chunk_z) = (quart_x >> 2, quart_z >> 2);
        self.chunk_with_status(chunk_x, chunk_z, ChunkStatus::Biomes)
            .access
            .get_noise_biome(quart_x, quart_y, quart_z)
    }

    /// `level.uncachedBiomeResolver()`: the generator's biome source with an uncached climate sampler.
    fn get_uncached_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self.env
            .biome_source()
            .create_uncached_resolver(&self.env.random_state)
            .get_noise_biome(quart_x, quart_y, quart_z)
    }

    fn mark_pos_for_post_processing(&mut self, pos: BlockPos) {
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.mark_pos_for_post_processing(pos);
        }
    }

    fn get_block_entity_nbt(&self, pos: BlockPos) -> Option<CompoundTag> {
        self.chunk(pos.x() >> 4, pos.z() >> 4)
            .access
            .get_block_entity_nbt(pos)
            .cloned()
    }

    fn set_block_entity_nbt(&mut self, tag: CompoundTag) {
        let pos = BlockPos::new(
            tag.get_int_or("x", 0),
            tag.get_int_or("y", 0),
            tag.get_int_or("z", 0),
        );
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.access.set_block_entity_nbt(tag);
        }
    }

    fn remove_block_entity_nbt(&mut self, pos: BlockPos) {
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.access.remove_block_entity(pos);
        }
    }

    fn add_structure_entity(&mut self, tag: &CompoundTag, pos: BlockPos, finalize: bool) -> bool {
        if self.live.is_none() {
            // a region bound to no live level has no registries or mob rules to create the entity with: the template's
            // saved NBT stays as it is, plus the random UUID the constructor would have drawn
            let mut tag = tag.clone();
            let (hi, lo) =
                mth::create_insecure_uuid(&mut mc_core::random::create_new()).as_u64_pair();
            tag.put_int_array(
                "UUID",
                vec![(hi >> 32) as i32, hi as i32, (lo >> 32) as i32, lo as i32],
            );
            return self.add_fresh_entity_nbt(tag);
        }
        let difficulty = finalize.then(|| WorldGenLevel::get_current_difficulty_at(self, pos));
        crate::world::entity::factory::add_structure_entity(tag, self, difficulty)
    }

    fn add_fresh_entity_nbt(&mut self, tag: CompoundTag) -> bool {
        let (x, z) = tag.get_list("Pos").map_or((0.0, 0.0), |pos| {
            (pos.get_double_or(0, 0.0), pos.get_double_or(2, 0.0))
        });
        let pos = BlockPos::new(mth::floor_f64(x), 0, mth::floor_f64(z));
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.add_entity(tag);
        }
        true
    }

    /// `BulkSectionAccess.getBlockState`: the section's block (AIR outside the build height), no read-zone warning.
    fn get_block_state_in_section(&self, pos: BlockPos) -> BlockState {
        if self.is_outside_build_height(pos.y()) {
            return Block::AIR.default_state();
        }
        let chunk = self.chunk(pos.x() >> 4, pos.z() >> 4);
        let index = self.region.level_height.section_index(pos.y()) as usize;
        chunk
            .access
            .get_section(index)
            .get_block_state(pos.x() & 15, pos.y() & 15, pos.z() & 15)
    }

    /// `section.setBlockState(x, y, z, state, false)`: raw palette write (read-only snapshots drop it).
    fn set_block_state_in_section(&mut self, pos: BlockPos, state: BlockState) {
        let index = self.region.level_height.section_index(pos.y()) as usize;
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.access.get_section_mut(index).set_block_state(
                pos.x() & 15,
                pos.y() & 15,
                pos.z() & 15,
                state,
            );
        }
    }

    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData> {
        self.region.structure_data(pos)
    }

    fn set_structure_start_tag(
        &mut self,
        pos: ChunkPos,
        structure_id: &str,
        tag: CompoundTag,
    ) -> bool {
        self.region.update_structure_start(pos, structure_id, tag)
    }

    fn is_upgrading(&self) -> bool {
        self.region
            .get_chunk(self.region.center.x, self.region.center.z)
            .is_some_and(ProtoChunk::is_upgrading)
    }
}

// ==== spawn framework additions (GenLevel as ServerLevelAccessor) ====

// ==== variant additions (GenLevel as SpawnContextLevel) ====

impl crate::world::entity::variant::SpawnContextLevel for GenLevel<'_> {
    fn variant_registries(&self) -> &crate::registry::RegistryAccess {
        crate::world::level::server_level_accessor::ServerLevelAccessor::registry_access(self)
    }

    fn biome_at(&self, pos: BlockPos) -> Option<Identifier> {
        Some(
            self.env
                .biomes
                .key(WorldGenLevel::get_biome(self, pos))
                .clone(),
        )
    }

    /// The moon brightness the live level published for generation (`with_difficulty`).
    fn moon_brightness_at(&self, _pos: BlockPos) -> f32 {
        self.difficulty.2
    }

    /// Vanilla reads the real level's chunks at `STRUCTURE_REFERENCES` without loading them (`getChunk(.., false)`); the
    /// chunks of a generation region are not published to the level yet, so nothing is found.
    fn has_structure_piece_at(
        &self,
        _pos: BlockPos,
        _matcher: &dyn Fn(&Identifier) -> bool,
    ) -> bool {
        false
    }
}

impl crate::world::level::server_level_accessor::ServerLevelAccessor for GenLevel<'_> {
    fn get_difficulty(&self) -> Difficulty {
        self.difficulty.0
    }

    fn get_current_difficulty_at(&self, pos: BlockPos) -> DifficultyInstance {
        WorldGenLevel::get_current_difficulty_at(self, pos)
    }

    // The worldgen region reads the real level's weather through `getLevel()`; not published to worker threads (spawn
    // predicates of chunk generation only read it for monsters, which do not spawn while generating).
    fn is_thundering(&self) -> bool {
        false
    }

    fn is_raining(&self) -> bool {
        false
    }

    fn has_ceiling(&self) -> bool {
        self.live
            .as_ref()
            .and_then(|l| l.dimension_type())
            .is_some_and(|d| d.has_ceiling)
    }

    fn monster_spawn_light_test(&self, random: &mut dyn RandomSource) -> i32 {
        match self.live.as_ref().and_then(|l| l.dimension_type()) {
            Some(dimension_type) => {
                crate::world::level::server_level_accessor::sample_int_provider(
                    dimension_type.monster_spawn_light_test(),
                    random,
                )
            }
            None => mc_worldgen::provider::IntProvider::Uniform {
                min_inclusive: 0,
                max_inclusive: 7,
            }
            .sample(random),
        }
    }

    fn monster_spawn_block_light_limit(&self) -> i32 {
        self.live
            .as_ref()
            .and_then(|l| l.dimension_type())
            .map_or(0, |d| d.monster_spawn_block_light_limit())
    }

    /// `WorldGenRegion.addFreshEntity`: the entity is saved into the chunk's entity list (vanilla `ProtoChunk.addEntity`).
    fn add_fresh_entity_with_passengers(
        &mut self,
        entity: Box<dyn crate::world::entity::Entity>,
    ) -> bool {
        let mut entity = entity;
        let pending = entity
            .mob_mut()
            .map(|m| std::mem::take(&mut m.pending_passengers))
            .unwrap_or_default();
        let lookup = |id| pending.iter().find(|p| p.id() == id).map(|p| &**p);
        match crate::world::entity::entity::save(&*entity, &lookup) {
            Some(tag) => self.add_fresh_entity_nbt(tag),
            None => false,
        }
    }

    fn no_collision_aabb(&self, aabb: &mc_core::phys::AABB) -> bool {
        use crate::world::level::collision::CollisionGetter as _;
        self.no_block_collision(None, aabb)
    }

    fn is_unobstructed_entity(&self, _entity: &dyn crate::world::entity::Entity) -> bool {
        true // a generation region has no entities
    }

    fn is_within_world_border(&self, _pos: BlockPos) -> bool {
        true // the border is not published to worker threads
    }

    fn contains_any_liquid(&self, aabb: &mc_core::phys::AABB) -> bool {
        crate::world::entity::movement::contains_any_liquid(self, aabb)
    }

    fn registry_access(&self) -> &crate::registry::RegistryAccess {
        static EMPTY: std::sync::LazyLock<crate::registry::RegistryAccess> =
            std::sync::LazyLock::new(Default::default);
        self.live.as_ref().map_or(&EMPTY, |l| l.registries())
    }

    fn get_biome_id(&self, pos: BlockPos) -> Option<BiomeId> {
        Some(WorldGenLevel::get_biome(self, pos))
    }

    fn spawn_mobs_rule(&self) -> bool {
        self.live.as_ref().is_none_or(|l| l.spawn_mobs())
    }

    fn create_entity(
        &self,
        ty: mc_data::EntityType,
        request: crate::world::entity::factory::EntitySpawnRequest,
    ) -> Option<Box<dyn crate::world::entity::Entity>> {
        let data = ty.data();
        if !request.ignore_checks {
            let required: Vec<Identifier> = data
                .required_features
                .iter()
                .filter_map(|n| Identifier::try_parse(n))
                .collect();
            let enabled =
                mc_world::level_storage::FeatureFlagSet::from_names(required.iter(), |_| {})
                    .is_subset_of(LevelReader::enabled_features(self));
            if !enabled || (!data.allowed_in_peaceful && self.difficulty.0 == Difficulty::Peaceful)
            {
                return None;
            }
        }
        if ty == mc_data::EntityType::PLAYER {
            return None;
        }
        Some(crate::world::entity::factory::instantiate(
            ty,
            self.live
                .as_ref()
                .and_then(|l| l.next_entity_id())
                .unwrap_or_else(crate::server::level::next_detached_entity_id),
            self.registry_access(),
        ))
    }

    fn with_random_taken(
        &mut self,
        f: &mut dyn FnMut(
            &dyn crate::world::level::server_level_accessor::ServerLevelAccessor,
            &mut dyn RandomSource,
        ) -> bool,
    ) -> bool {
        let mut random = std::mem::replace(
            &mut self.random,
            Box::new(mc_core::random::LegacyRandomSource::new(0)),
        );
        let result = f(&*self, &mut *random);
        self.random = random;
        result
    }

    fn natural_mob_spawns_at(
        &self,
        pos: BlockPos,
    ) -> Arc<crate::world::level::spawner::mob_spawn_settings::MobSpawnSettings> {
        let base = self.spawn_base();
        crate::world::level::spawner::biome_spawns::BiomeSpawnTable::for_registry(&self.env.biomes)
            .natural_mob_spawns(&base, Some(WorldGenLevel::get_biome(self, pos)))
    }

    fn creature_world_gen_spawn_probability_at(&self, pos: BlockPos) -> f32 {
        let base = self.spawn_base();
        crate::world::level::spawner::biome_spawns::BiomeSpawnTable::for_registry(&self.env.biomes)
            .creature_world_gen_spawn_probability(&base, Some(WorldGenLevel::get_biome(self, pos)))
    }
}

impl GenLevel<'_> {
    /// The dimension type's constant layer of the spawning attributes.
    fn spawn_base(&self) -> crate::world::level::spawner::biome_spawns::DimensionSpawnBase {
        match self.live.as_ref().and_then(|l| l.dimension_type()) {
            Some(dimension_type) => {
                crate::world::level::spawner::biome_spawns::DimensionSpawnBase::from_attributes(
                    &dimension_type.attributes,
                )
            }
            None => Default::default(),
        }
    }
}

/// A generation region has no entities and no border collision: only the block collisions of `CollisionGetter`.
impl crate::world::level::collision::CollisionGetter for GenLevel<'_> {}
