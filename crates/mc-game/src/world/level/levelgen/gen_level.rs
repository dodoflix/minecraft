//! `GenLevel`: vanilla's `WorldGenRegion` (the `WorldGenLevel` features, placements, structures and block behaviours run
//! against during chunk generation) over the chunks lent to one generation step ([`WorldGenRegion`] of
//! `server::level::world_gen`).
//!
//! Semantics follow `WorldGenRegion` exactly:
//! * reads (`getBlockState`, `getFluidState`, `getHeight`, ...) outside the step's block-state write zone log vanilla's
//!   `Detected unsafe terrain read during worldgen` error (and continue); a chunk the step may not see is the crash
//!   `Requested chunk unavailable during world generation` (a panic here);
//! * writes go through `ensureCanWrite` (`Detected setBlock in a far chunk` + dropped outside the write zone, the
//!   upgrading-chunk height rule), then `ProtoChunk.setBlockState` (heightmaps per the chunk's status, light engine and
//!   POI untouched), the `DUMMY` block entity NBT of block-entity states (the real NBT arrives through
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
use crate::server::level::world_gen::{GenChunk, WorldGenRegion};
use crate::world::block_entity::BlockEntity;
use crate::world::difficulty_instance::DifficultyInstance;
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
    /// Heightmaps read while missing from their (writable) chunk: `(chunkX, chunkZ, type)`, primed lazily.
    pending_heightmaps: RefCell<Vec<(i32, i32, HeightmapType)>>,
}

impl<'a> GenLevel<'a> {
    /// `new WorldGenRegion(level, cache, step, center)` for the feature step (`ChunkStatus.FEATURES`).
    pub fn new(region: &'a mut WorldGenRegion, env: Arc<GenEnv>) -> GenLevel<'a> {
        let random = env
            .random_state
            .get_or_create_random_factory(&Identifier::with_default_namespace("worldgen_region_random").expect("valid identifier"))
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
    pub fn with_difficulty(mut self, difficulty: Difficulty, overworld_clock_time: i64, moon_brightness: f32) -> GenLevel<'a> {
        self.difficulty = (difficulty, overworld_clock_time, moon_brightness);
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
            if self.region.gen_chunk(chunk_x, chunk_z).is_some_and(GenChunk::is_writable) {
                let chunk = self.region.get_chunk_mut_unchecked(ChunkPos::new(chunk_x, chunk_z));
                if !chunk.access.has_primed_heightmap(ty) {
                    chunk.access.prime_heightmaps(true, &[ty]);
                }
            }
        }
    }

    fn max_allowed_status(&self, distance: i32) -> Option<ChunkStatus> {
        match &self.max_allowed {
            Some(list) => usize::try_from(distance).ok().and_then(|d| list.get(d)).copied(),
            None => (distance <= self.region.read_radius).then_some(ChunkStatus::Full),
        }
    }

    /// `getChunk(chunkX, chunkZ, targetStatus)`: the crash report of vanilla (a panic) when the step may not see the chunk.
    fn chunk_with_status(&self, chunk_x: i32, chunk_z: i32, target_status: ChunkStatus) -> &ProtoChunk {
        let distance = self.region.center.get_chessboard_distance_xz(chunk_x, chunk_z);
        let max_allowed = self.max_allowed_status(distance);
        let chunk = if max_allowed.is_some() { self.region.get_chunk(chunk_x, chunk_z) } else { None };
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
            chunk.map_or_else(|| "[out of cache bounds]".to_string(), |c| c.get_persisted_status().name()),
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
        if self.region.gen_chunk(chunk_x, chunk_z).is_some_and(GenChunk::is_writable) {
            Some(self.region.get_chunk_mut_unchecked(ChunkPos::new(chunk_x, chunk_z)))
        } else {
            None
        }
    }

    fn is_within_write_zone_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        let center = self.region.center;
        (center.x - chunk_x).abs() <= self.region.write_radius && (center.z - chunk_z).abs() <= self.region.write_radius
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
        if (center.x != chunk_x || center.z != chunk_z) && !self.is_within_write_zone_chunk(chunk_x, chunk_z) {
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
        let tick = ScheduledTick { ty, pos, trigger_tick: self.game_time.wrapping_add(i64::from(delay)), priority, sub_tick_order };
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
    /// `hasChunk`: `distance < directDependencies.size()`.
    fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.max_allowed_status(self.region.center.get_chessboard_distance_xz(chunk_x, chunk_z)).is_some()
    }

    /// `getHeight(type, x, z)`: the chunk's heightmap value `+ 1`.
    fn get_height_at(&self, ty: HeightmapType, x: i32, z: i32) -> i32 {
        let (chunk_x, chunk_z) = (x >> 4, z >> 4);
        self.warn_if_read_outside_write_zone(chunk_x, chunk_z);
        let chunk = self.chunk(chunk_x, chunk_z);
        let ty = if self.region.gen_chunk(chunk_x, chunk_z).is_some_and(GenChunk::is_writable) { ty } else { Self::fix_type(ty) };
        match chunk.access.heightmaps.get(&ty) {
            Some(heightmap) => heightmap.get_first_available(x & 15, z & 15),
            None => {
                if self.region.gen_chunk(chunk_x, chunk_z).is_some_and(GenChunk::is_writable) {
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

    /// The light engine is not touched during generation: light is not computed before `INITIALIZE_LIGHT`.
    fn get_brightness(&self, _layer: LightLayer, _pos: BlockPos) -> i32 {
        0
    }

    fn get_sky_darken(&self) -> i32 {
        0
    }

    fn get_raw_brightness(&self, _pos: BlockPos, _sky_darkening: i32) -> i32 {
        0
    }

    fn can_see_sky(&self, _pos: BlockPos) -> bool {
        false
    }
}

impl LevelAccessor for GenLevel<'_> {
    fn mark_for_post_processing(&mut self, pos: BlockPos) {
        WorldGenLevel::mark_pos_for_post_processing(self, pos);
    }
    fn set_block(&mut self, pos: BlockPos, state: BlockState, flags: u32, _update_limit: i32) -> bool {
        if !self.ensure_can_write(pos) {
            return false;
        }
        self.flush_heightmaps();
        if let Some(chunk) = self.chunk_for_write(pos) {
            // (`updatePOIOnBlockStateChange` has no effect on proto chunks' generation output)
            let old_state = chunk.set_block_state(pos, state, None);
            if state.has_block_entity() {
                // A ProtoChunk never has the LEVELCHUNK type: the placeholder the real block entity NBT replaces. A live
                // block entity (anything but the placeholder: vanilla keeps the instance in `ProtoChunk.blockEntities`, the
                // placeholder only goes to the pending NBT) survives the block being set again.
                let live = chunk.access.get_block_entity_nbt(pos).is_some_and(|tag| tag.get_string("id") != Some("DUMMY"));
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

    fn destroy_block(&mut self, pos: BlockPos, _drop_block: bool, _breaker: Option<i32>, update_limit: i32) -> bool {
        if self.get_block_state(pos).is_air() { false } else { self.set_block(pos, Block::AIR.default_state(), 3, update_limit) }
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

    fn schedule_block_tick(&mut self, pos: BlockPos, block: Block, delay: i32, priority: TickPriority) {
        self.schedule_tick(pos, block, delay, priority, |chunk| &mut chunk.block_ticks);
    }

    fn schedule_fluid_tick(&mut self, pos: BlockPos, fluid: Fluid, delay: i32, priority: TickPriority) {
        self.schedule_tick(pos, fluid, delay, priority, |chunk| &mut chunk.fluid_ticks);
    }

    fn has_scheduled_block_tick(&self, pos: BlockPos, block: Block) -> bool {
        self.region_chunk_ticks(pos, |chunk| chunk.block_ticks.has_scheduled_tick(&pos, block))
    }

    fn has_scheduled_fluid_tick(&self, pos: BlockPos, fluid: Fluid) -> bool {
        self.region_chunk_ticks(pos, |chunk| chunk.fluid_ticks.has_scheduled_tick(&pos, fluid))
    }

    /// No neighbor updates during generation.
    fn update_neighbors_at(&mut self, _pos: BlockPos, _source_block: Block) {}
}

impl GenLevel<'_> {
    /// `getChunk(pos).getBlockTicks()` lookups: read-only snapshots have the black hole (never scheduled).
    fn region_chunk_ticks(&self, pos: BlockPos, f: impl FnOnce(&ProtoChunk) -> bool) -> bool {
        let (chunk_x, chunk_z) = (pos.x() >> 4, pos.z() >> 4);
        let chunk = self.chunk(chunk_x, chunk_z);
        self.region.gen_chunk(chunk_x, chunk_z).is_some_and(GenChunk::is_writable) && f(chunk)
    }
}

impl WorldGenLevel for GenLevel<'_> {
    fn get_seed(&self) -> i64 {
        self.env.seed
    }

    fn get_current_difficulty_at(&self, pos: BlockPos) -> DifficultyInstance {
        assert!(self.has_chunk(pos.x() >> 4, pos.z() >> 4), "We are asking a region for a chunk out of bound");
        let (difficulty, clock_time, moon_brightness) = self.difficulty;
        DifficultyInstance::new(difficulty, clock_time, 0, moon_brightness)
    }

    fn env(&self) -> &Arc<GenEnv> {
        &self.env
    }

    fn get_center(&self) -> ChunkPos {
        self.region.center
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
            return !center.get_height_accessor_for_generation().is_outside_build_height(pos.y());
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
        self.chunk_with_status(chunk_x, chunk_z, ChunkStatus::Biomes).access.get_noise_biome(quart_x, quart_y, quart_z)
    }

    /// `level.uncachedBiomeResolver()`: the generator's biome source with an uncached climate sampler.
    fn get_uncached_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        self.env.biome_source().create_uncached_resolver(&self.env.random_state).get_noise_biome(quart_x, quart_y, quart_z)
    }

    fn mark_pos_for_post_processing(&mut self, pos: BlockPos) {
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.mark_pos_for_post_processing(pos);
        }
    }

    fn get_block_entity_nbt(&self, pos: BlockPos) -> Option<&CompoundTag> {
        self.chunk(pos.x() >> 4, pos.z() >> 4).access.get_block_entity_nbt(pos)
    }

    fn set_block_entity_nbt(&mut self, tag: CompoundTag) {
        let pos = BlockPos::new(tag.get_int_or("x", 0), tag.get_int_or("y", 0), tag.get_int_or("z", 0));
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.access.set_block_entity_nbt(tag);
        }
    }

    fn remove_block_entity_nbt(&mut self, pos: BlockPos) {
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.access.remove_block_entity(pos);
        }
    }

    fn add_fresh_entity_nbt(&mut self, tag: CompoundTag) -> bool {
        let (x, z) = tag.get_list("Pos").map_or((0.0, 0.0), |pos| (pos.get_double_or(0, 0.0), pos.get_double_or(2, 0.0)));
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
        chunk.access.get_section(index).get_block_state(pos.x() & 15, pos.y() & 15, pos.z() & 15)
    }

    /// `section.setBlockState(x, y, z, state, false)`: raw palette write (read-only snapshots drop it).
    fn set_block_state_in_section(&mut self, pos: BlockPos, state: BlockState) {
        let index = self.region.level_height.section_index(pos.y()) as usize;
        if let Some(chunk) = self.chunk_for_write(pos) {
            chunk.access.get_section_mut(index).set_block_state(pos.x() & 15, pos.y() & 15, pos.z() & 15, state);
        }
    }

    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData> {
        self.region.structure_data(pos)
    }

    fn set_structure_start_tag(&mut self, pos: ChunkPos, structure_id: &str, tag: CompoundTag) -> bool {
        if !self.region.gen_chunk(pos.x, pos.z).is_some_and(GenChunk::is_writable) {
            return false;
        }
        let chunk = self.region.get_chunk_mut_unchecked(pos);
        match chunk.access.structures.starts.get_mut(structure_id) {
            Some(slot) => {
                *slot = tag;
                chunk.access.mark_unsaved();
                true
            }
            None => false,
        }
    }

    fn is_upgrading(&self) -> bool {
        self.region.get_chunk(self.region.center.x, self.region.center.z).is_some_and(ProtoChunk::is_upgrading)
    }
}
