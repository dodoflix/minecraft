//! `WorldGenLevel`: the level interface features, placements, structures and block behaviours run against during chunk
//! generation (`WorldGenRegion` in vanilla). Implemented by [`GenLevel`](super::gen_level::GenLevel) over the chunks lent
//! to a generation step.
//!
//! Reads/writes go through [`LevelAccessor`]; the reads outside the write zone log vanilla's "unsafe terrain read" warning
//! and writes outside it are dropped with vanilla's error log (`ensureCanWrite`). Block entities are the chunk's raw NBT
//! (`DUMMY` entries written by `set_block`, replaced by [`WorldGenLevel::set_block_entity_nbt`]); live block entity
//! instances only exist after the chunk is promoted to a `LevelChunk`.

use super::env::GenEnv;
use crate::world::difficulty_instance::DifficultyInstance;
use crate::world::level::LevelAccessor;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_data::block::{Block, BlockState};
use mc_nbt::CompoundTag;
use mc_world::chunk::StructureData;
use mc_world::palette::BiomeId;
use std::sync::Arc;

pub trait WorldGenLevel: LevelAccessor {
    /// `getSeed()`.
    fn get_seed(&self) -> i64;
    /// `WorldGenRegion.getCurrentDifficultyAt(pos)`: `new DifficultyInstance(level.getDifficulty(), level.getOverworldClockTime(),
    /// 0, level.getMoonBrightness(pos))` (panics like vanilla's `RuntimeException` for a chunk outside the region).
    fn get_current_difficulty_at(&self, pos: BlockPos) -> DifficultyInstance;
    /// The level's generator environment (`getLevel().getChunkSource().getGenerator()`, registries, ...).
    fn env(&self) -> &Arc<GenEnv>;
    /// `ensureCanWrite(pos)`: inside the step's block-state write radius (logs vanilla's error otherwise).
    fn ensure_can_write(&mut self, pos: BlockPos) -> bool;
    /// `isWithinWriteZone(pos)` (no logging).
    fn is_within_write_zone(&self, pos: BlockPos) -> bool;
    /// `setCurrentlyGenerating(supplier)`: names the feature/structure for warnings.
    fn set_currently_generating(&mut self, what: Option<Box<dyn Fn() -> String + Send + Sync>>);
    /// `getBiome(pos)` (`BiomeManager` zoom over the noise biomes of the chunks).
    fn get_biome(&self, pos: BlockPos) -> BiomeId;
    /// `getNoiseBiome(quartX, quartY, quartZ)` of the region's chunks.
    fn get_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId;
    /// `getUncachedNoiseBiome(..)`: the generator's own biome source (not the chunk contents).
    fn get_uncached_noise_biome(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> BiomeId;
    /// `getChunk(pos).markPosForPostProcessing(pos)`.
    fn mark_pos_for_post_processing(&mut self, pos: BlockPos);
    /// `getChunk(pos).getBlockEntityNbt(pos)`.
    fn get_block_entity_nbt(&self, pos: BlockPos) -> Option<CompoundTag>;
    /// `getChunk(pos).setBlockEntityNbt(tag)` (position from the tag's `x`/`y`/`z`).
    fn set_block_entity_nbt(&mut self, tag: CompoundTag);
    /// `getChunk(pos).removeBlockEntity(pos)`.
    fn remove_block_entity_nbt(&mut self, pos: BlockPos);
    /// `addFreshEntity(entity)` for an entity given as saved NBT (`id`, `Pos`, ...): `chunk.addEntity(tag)`.
    fn add_fresh_entity_nbt(&mut self, tag: CompoundTag) -> bool;
    /// The entity half of `StructureTemplate.placeEntities`: the entity of `tag` created, rotated, finalized
    /// (`Mob.finalizeSpawn` at `pos`) when `finalize` and added with its passengers; false when it cannot be created.
    fn add_structure_entity(&mut self, tag: &CompoundTag, pos: BlockPos, finalize: bool) -> bool;
    /// Structure starts/references of any chunk within reach (`StructureManager.startsForStructure` source).
    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData>;
    /// Replaces the raw start of `structure_id` in the chunk `pos` with `tag` (a piece mutated its state while being
    /// placed, e.g. `ScatteredFeaturePiece.heightPosition`; vanilla shares the start object with the chunk, which saves it
    /// later). Returns whether the chunk accepted the write (it is writable in this region); levels that cannot persist it
    /// keep the default.
    fn set_structure_start_tag(
        &mut self,
        _pos: ChunkPos,
        _structure_id: &str,
        _tag: CompoundTag,
    ) -> bool {
        false
    }
    /// `BulkSectionAccess.getBlockState(pos)`: the block of the chunk section at `pos` read directly (AIR outside the build
    /// height); no write-zone check and no "unsafe terrain read" warning. Default: `BlockGetter::get_block_state`.
    fn get_block_state_in_section(&self, pos: BlockPos) -> BlockState {
        if self.is_outside_build_height(pos.y()) {
            Block::AIR.default_state()
        } else {
            self.get_block_state(pos)
        }
    }
    /// `BulkSectionAccess.getSection(pos).setBlockState(sectionRelative x/y/z, state, false)`: a raw section write (no
    /// heightmap, light, block entity, neighbour or post-processing update; `pos` is inside the build height and the write
    /// zone). Implementors over real chunks MUST override; the default is a plain flag-less `setBlock`.
    fn set_block_state_in_section(&mut self, pos: BlockPos, state: BlockState) {
        self.set_block(pos, state, 0, 0);
    }
    /// Makes sure every chunk within one block of `area` is available for reading: a live level loads them like vanilla's
    /// `getChunk` does on first touch; generation regions have their chunks already.
    fn ensure_chunks_loaded_around(&mut self, _area: &mc_core::bounding_box::BoundingBox) {}
    /// `chunk.isUpgrading()` of the center chunk.
    fn is_upgrading(&self) -> bool {
        false
    }
}
