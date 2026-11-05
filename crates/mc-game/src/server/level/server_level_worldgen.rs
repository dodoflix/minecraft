//! `ServerLevel` as a `WorldGenLevel` (vanilla: `ServerLevel implements WorldGenLevel`): what `/place`, `/test` and structure
//! blocks run features, structures and templates against. Writes are the level's own `setBlock` (live block entities,
//! neighbour/shape updates, light); reads that need an unloaded chunk see air like every `&self` read of `ServerLevel`, so
//! callers that need vanilla's load-on-read call [`ServerLevel::ensure_chunks_loaded`] for the area first.
//!
//! Block entity types without an implementation are kept as the chunk's pending raw NBT (the level's save path writes
//! them back), a block entity block without any entry reads as the `DUMMY` stub `GenLevel` uses.

use super::ServerLevel;
use crate::server::MinecraftServer;
use crate::world::block_entity::BlockEntity;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::entity::factory::{EntitySpawnRequest, load_entity_recursive};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::{GenEnv, WorldGenLevel};
use crate::world::level::{BlockGetter, DimensionKey, LevelAccessor};
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_data::enums::EntitySpawnReason;
use mc_nbt::CompoundTag;
use mc_world::chunk::{StructureData, pack_offset_coordinates};
use mc_world::palette::BiomeId;
use mc_worldgen::biome::BiomeManager;
use std::sync::Arc;

/// What `getCurrentDifficultyAt` needs from the server (`getOverworldClockTime()`, `getMoonBrightness(pos)`).
#[derive(Clone, Copy, Debug, Default)]
pub struct WorldGenClock {
    pub overworld_clock_time: i64,
    pub moon_brightness: f32,
}

impl ServerLevel {
    /// `getLevel().getChunkSource().getGenerator()` and friends: the generation environment of this level.
    pub fn gen_env(&self) -> &Arc<GenEnv> {
        self.gen_env.as_ref().expect("this level has no generation environment")
    }

    /// `getStructureTemplateManager()`.
    pub fn structure_template_manager(&self) -> &Arc<crate::world::level::levelgen::structure::templatesystem::StructureTemplateManager> {
        &self.gen_env().structure_template_manager
    }

    /// Loads every full chunk of the inclusive range (vanilla blocks on `getChunk` for whatever a feature touches).
    pub fn ensure_chunks_loaded(&mut self, min: ChunkPos, max: ChunkPos) {
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                if self.chunk_now(x, z).is_none() {
                    self.chunk_blocking_mut(x, z);
                }
            }
        }
    }
}

/// Runs `f` on the level of `dimension` with the server clock inputs of `getCurrentDifficultyAt` filled in (the level is out
/// of the server's map meanwhile, like during its tick).
pub fn with_worldgen_level<R>(server: &mut MinecraftServer, dimension: &DimensionKey, f: impl FnOnce(&mut ServerLevel) -> R) -> Option<R> {
    use crate::server::level::level_tick::LevelTickEnv as _;
    server.with_level(dimension, |server, level| {
        let env = crate::server::level_tick_env::ServerTickEnv { server, dimension: dimension.clone() };
        level.worldgen_clock = Some(WorldGenClock { overworld_clock_time: env.overworld_clock_time(), moon_brightness: level.get_moon_brightness(&env) });
        let result = f(level);
        level.worldgen_clock = None;
        result
    })
}

impl WorldGenLevel for ServerLevel {
    fn get_seed(&self) -> i64 {
        self.gen_env().seed
    }

    /// `Level.getCurrentDifficultyAt(pos)`.
    fn get_current_difficulty_at(&self, pos: BlockPos) -> DifficultyInstance {
        let clock = self.worldgen_clock.unwrap_or_default();
        let (local_time, moon) = match self.chunk_now(pos.x() >> 4, pos.z() >> 4) {
            Some(chunk) => (chunk.data.access.inhabited_time(), clock.moon_brightness),
            None => (0, 0.0),
        };
        DifficultyInstance::new(self.level_data.get_difficulty(), clock.overworld_clock_time, local_time, moon)
    }

    fn env(&self) -> &Arc<GenEnv> {
        self.gen_env()
    }

    fn ensure_can_write(&mut self, _pos: BlockPos) -> bool {
        true
    }

    fn is_within_write_zone(&self, _pos: BlockPos) -> bool {
        true
    }

    fn set_currently_generating(&mut self, _what: Option<Box<dyn Fn() -> String + Send + Sync>>) {}

    fn get_biome(&self, pos: BlockPos) -> BiomeId {
        BiomeManager::new(&|qx: i32, qy: i32, qz: i32| WorldGenLevel::get_noise_biome(self, qx, qy, qz), self.tick_state.biome_zoom_seed).get_biome(pos)
    }

    /// The stored biome of a loaded chunk; the generator's biome source for a chunk that is not loaded (what vanilla's
    /// load-on-read would have generated).
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        match self.chunk_now(quart_x >> 2, quart_z >> 2) {
            Some(chunk) => chunk.data.access.get_noise_biome(quart_x, quart_y, quart_z),
            None => self.get_uncached_noise_biome(quart_x, quart_y, quart_z),
        }
    }

    fn get_uncached_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId {
        let env = self.gen_env();
        env.biome_source().create_uncached_resolver(&env.random_state).get_noise_biome(quart_x, quart_y, quart_z)
    }

    fn mark_pos_for_post_processing(&mut self, pos: BlockPos) {
        if let Some(chunk) = self.chunk_now_mut(pos.x() >> 4, pos.z() >> 4)
            && chunk.data.access.level_height.is_inside_build_height(pos.y())
        {
            let index = chunk.data.access.level_height.section_index(pos.y()) as usize;
            chunk.data.access.post_processing[index].get_or_insert_with(Vec::new).push(pack_offset_coordinates(pos));
            chunk.data.access.mark_unsaved();
        }
    }

    fn get_block_entity_nbt(&self, pos: BlockPos) -> Option<CompoundTag> {
        let chunk = self.chunk_now(pos.x() >> 4, pos.z() >> 4)?;
        if let Some(block_entity) = chunk.block_entities.get(&pos).filter(|be| !be.is_removed()) {
            return Some(block_entity.save_with_full_metadata());
        }
        if let Some(tag) = chunk.data.access.get_block_entity_nbt(pos) {
            return Some(tag.clone());
        }
        BlockGetter::get_block_state(self, pos).has_block_entity().then(|| {
            let mut tag = CompoundTag::new();
            tag.put_int("x", pos.x());
            tag.put_int("y", pos.y());
            tag.put_int("z", pos.z());
            tag.put_string("id", "DUMMY");
            tag
        })
    }

    /// `blockEntity.loadWithComponents(tag)` on the live block entity, or the raw NBT kept for a type without an
    /// implementation.
    fn set_block_entity_nbt(&mut self, tag: CompoundTag) {
        let pos = BlockPos::new(tag.get_int_or("x", 0), tag.get_int_or("y", 0), tag.get_int_or("z", 0));
        if let Some(block_entity) = self.get_block_entity_immediate(pos) {
            block_entity.load_with_components(&tag);
            self.block_entity_changed(pos);
        } else if let Some(chunk) = self.chunk_now_mut(pos.x() >> 4, pos.z() >> 4) {
            chunk.data.access.set_block_entity_nbt(tag);
            chunk.data.access.mark_unsaved();
        }
    }

    fn remove_block_entity_nbt(&mut self, pos: BlockPos) {
        self.remove_block_entity(pos);
        if let Some(chunk) = self.chunk_now_mut(pos.x() >> 4, pos.z() >> 4) {
            chunk.data.access.remove_block_entity(pos);
        }
    }

    /// `addFreshEntityWithPassengers` of the entity `EntityType.create(tag, level, STRUCTURE)` loads.
    fn add_fresh_entity_nbt(&mut self, tag: CompoundTag) -> bool {
        let mut loaded = Vec::new();
        load_entity_recursive(&tag, self, EntitySpawnRequest { reason: EntitySpawnReason::Structure, ignore_checks: false }, &mut loaded);
        if loaded.is_empty() {
            return false;
        }
        self.add_fresh_entity_with_passengers(loaded);
        true
    }

    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData> {
        self.chunk_now(pos.x, pos.z).map(|chunk| &chunk.data.access.structures)
    }

    /// The chunk shares the start object with the placing piece in vanilla: persist the changed state.
    fn set_structure_start_tag(&mut self, pos: ChunkPos, structure_id: &str, tag: CompoundTag) -> bool {
        let Some(chunk) = self.chunk_now_mut(pos.x, pos.z) else { return false };
        chunk.data.access.structures.starts.insert(structure_id.to_owned(), tag);
        chunk.data.access.mark_unsaved();
        true
    }

    fn get_block_state_in_section(&self, pos: BlockPos) -> BlockState {
        if self.is_outside_build_height(pos.y()) {
            return Block::AIR.default_state();
        }
        match self.chunk_now(pos.x() >> 4, pos.z() >> 4) {
            Some(chunk) => {
                let index = chunk.data.access.level_height.section_index(pos.y()) as usize;
                chunk.data.access.get_section(index).get_block_state(pos.x() & 15, pos.y() & 15, pos.z() & 15)
            }
            None => Block::AIR.default_state(),
        }
    }

    fn set_block_state_in_section(&mut self, pos: BlockPos, state: BlockState) {
        if let Some(chunk) = self.chunk_now_mut(pos.x() >> 4, pos.z() >> 4) {
            let index = chunk.data.access.level_height.section_index(pos.y()) as usize;
            chunk.data.access.get_section_mut(index).set_block_state(pos.x() & 15, pos.y() & 15, pos.z() & 15, state);
        }
    }
}
