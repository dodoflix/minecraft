//! `WorldGenLevel`: the level interface features, placements, structures and block behaviours run against during chunk
//! generation (`WorldGenRegion` in vanilla). Implemented by [`GenLevel`](super::gen_level::GenLevel) over the chunks lent
//! to a generation step.
//!
//! Reads/writes go through [`LevelAccessor`]; the reads outside the write zone log vanilla's "unsafe terrain read" warning
//! and writes outside it are dropped with vanilla's error log (`ensureCanWrite`). Block entities are the chunk's raw NBT
//! (`DUMMY` entries written by `set_block`, replaced by [`WorldGenLevel::set_block_entity_nbt`]); live block entity
//! instances only exist after the chunk is promoted to a `LevelChunk`.

use super::env::GenEnv;
use crate::world::level::LevelAccessor;
use mc_core::pos::{BlockPos, ChunkPos};
use mc_nbt::CompoundTag;
use mc_world::chunk::StructureData;
use mc_world::palette::BiomeId;
use std::sync::Arc;

pub trait WorldGenLevel: LevelAccessor {
    /// `getSeed()`.
    fn get_seed(&self) -> i64;
    /// The level's generator environment (`getLevel().getChunkSource().getGenerator()`, registries, ...).
    fn env(&self) -> &Arc<GenEnv>;
    /// `WorldGenRegion.getCenter()`: the chunk being generated.
    fn get_center(&self) -> ChunkPos;
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
    fn get_block_entity_nbt(&self, pos: BlockPos) -> Option<&CompoundTag>;
    /// `getChunk(pos).setBlockEntityNbt(tag)` (position from the tag's `x`/`y`/`z`).
    fn set_block_entity_nbt(&mut self, tag: CompoundTag);
    /// `getChunk(pos).removeBlockEntity(pos)`.
    fn remove_block_entity_nbt(&mut self, pos: BlockPos);
    /// `addFreshEntity(entity)` for an entity given as saved NBT (`id`, `Pos`, ...): `chunk.addEntity(tag)`.
    fn add_fresh_entity_nbt(&mut self, tag: CompoundTag) -> bool;
    /// Structure starts/references of any chunk within reach (`StructureManager.startsForStructure` source).
    fn structure_data(&self, pos: ChunkPos) -> Option<&StructureData>;
    /// `chunk.isUpgrading()` of the center chunk.
    fn is_upgrading(&self) -> bool {
        false
    }
}
