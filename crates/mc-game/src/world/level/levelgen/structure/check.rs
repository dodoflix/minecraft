//! `StructureCheck` + `StructureCheckResult`: the cheap "is there a start for structure S in chunk C" test used by
//! `/locate` and explorer maps, answered from loaded starts, the region files (`ChunkScanAccess`) or, failing both, by
//! running the structure's generation point search.

use super::structure::{GenerationContext, Structure};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use mc_core::ident::Identifier;
use mc_core::pos::ChunkPos;
use mc_nbt::CompoundTag;
use mc_world::level_storage::{DATA_VERSION_TAG, WORLD_VERSION};
use mc_worldgen::structure_placement::StructurePlacement;
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Mutex;

/// `StructureCheckResult`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureCheckResult {
    StartPresent,
    StartNotPresent,
    ChunkLoadNeeded,
}

/// `ChunkScanAccess.scanChunk(pos, ..)`: the stored (not datafixed) chunk tag of an unloaded chunk, `None` if the
/// chunk was never saved.
pub trait ChunkScanAccess: Send + Sync {
    fn scan_chunk(&self, pos: ChunkPos) -> io::Result<Option<CompoundTag>>;
}

/// A storage without chunks (every scan finds nothing).
pub struct NoChunkScan;

impl ChunkScanAccess for NoChunkScan {
    fn scan_chunk(&self, _pos: ChunkPos) -> io::Result<Option<CompoundTag>> {
        Ok(None)
    }
}

const MAX_CHUNKS_WITHOUT_STORED_STARTS: usize = 131072;

#[derive(Default)]
struct State {
    /// `loadedChunks`: chunk -> structure id -> reference count of its valid start.
    loaded_chunks: HashMap<i64, HashMap<Identifier, i32>>,
    /// `featureChecks`: structure id -> chunk -> `canCreateStructure`.
    feature_checks: HashMap<Identifier, HashMap<i64, bool>>,
    chunks_without_starts_in_storage: HashSet<i64>,
}

impl State {
    /// `storeFullResults(posKey, starts)`.
    fn store_full_results(&mut self, pos_key: i64, starts: HashMap<Identifier, i32>) {
        self.loaded_chunks.insert(pos_key, starts);
        for m in self.feature_checks.values_mut() {
            m.remove(&pos_key);
        }
        self.chunks_without_starts_in_storage.remove(&pos_key);
    }
}

/// `StructureCheck`.
pub struct StructureCheck {
    storage_access: Box<dyn ChunkScanAccess>,
    state: Mutex<State>,
}

/// `checkStructureInfo(cachedResult, structure, requireUnreferenced)`.
fn check_structure_info(cached: &HashMap<Identifier, i32>, structure: &Identifier, require_unreferenced: bool) -> StructureCheckResult {
    let reference_count = cached.get(structure).copied().unwrap_or(-1);
    if reference_count == -1 || (require_unreferenced && reference_count != 0) {
        StructureCheckResult::StartNotPresent
    } else {
        StructureCheckResult::StartPresent
    }
}

impl StructureCheck {
    pub fn new(storage_access: Box<dyn ChunkScanAccess>) -> StructureCheck {
        StructureCheck { storage_access, state: Mutex::new(State::default()) }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `checkStart(pos, structure, placement, requireUnreferenced)`.
    pub fn check_start(&self, env: &GenEnv, pos: ChunkPos, structure: &Holder<dyn Structure>, placement: &StructurePlacement, require_unreferenced: bool) -> StructureCheckResult {
        let pos_key = pos.pack();
        let Some(id) = structure.id() else { return StructureCheckResult::StartNotPresent };
        {
            let state = self.state();
            if let Some(cached) = state.loaded_chunks.get(&pos_key) {
                return check_structure_info(cached, id, require_unreferenced);
            }
        }
        let without_starts = self.state().chunks_without_starts_in_storage.contains(&pos_key);
        if !without_starts {
            if let Some(result) = self.try_load_from_storage(env, pos, id, require_unreferenced) {
                return result;
            }
            let mut state = self.state();
            if state.chunks_without_starts_in_storage.len() >= MAX_CHUNKS_WITHOUT_STORED_STARTS {
                state.chunks_without_starts_in_storage.clear();
            }
            state.chunks_without_starts_in_storage.insert(pos_key);
        }
        if !placement.apply_additional_chunk_restrictions(pos.x, pos.z, env.seed) {
            return StructureCheckResult::StartNotPresent;
        }
        let cached = self.state().feature_checks.get(id).and_then(|m| m.get(&pos_key)).copied();
        let is_feature_chunk = match cached {
            Some(v) => v,
            None => {
                let v = Self::can_create_structure(env, pos, structure);
                self.state().feature_checks.entry(id.clone()).or_default().insert(pos_key, v);
                v
            }
        };
        if is_feature_chunk { StructureCheckResult::ChunkLoadNeeded } else { StructureCheckResult::StartNotPresent }
    }

    /// `canCreateStructure(pos, structure)`.
    fn can_create_structure(env: &GenEnv, pos: ChunkPos, structure: &Holder<dyn Structure>) -> bool {
        let value = structure.value();
        let valid_biome = |biome| value.biomes().contains(&biome);
        let mut context = GenerationContext::new(env, env.seed, pos, env.level_height, &valid_biome);
        value.find_valid_generation_point(&mut context).is_some()
    }

    /// `tryLoadFromStorage(pos, structure, requireUnreferenced)`.
    fn try_load_from_storage(&self, env: &GenEnv, pos: ChunkPos, structure: &Identifier, require_unreferenced: bool) -> Option<StructureCheckResult> {
        let chunk_tag = match self.storage_access.scan_chunk(pos) {
            Ok(Some(tag)) => tag,
            Ok(None) => return None,
            Err(e) => {
                log::warn!("Failed to read chunk {pos}: {e}");
                return Some(StructureCheckResult::ChunkLoadNeeded);
            }
        };
        // Gap: chunks from older data versions are not datafixed; their `Level.Structures.Starts` is read as is.
        let starts = if chunk_tag.get_int_or(DATA_VERSION_TAG, 0) < WORLD_VERSION {
            chunk_tag.get_compound("structures").and_then(|t| t.get_compound("starts")).or_else(|| chunk_tag.get_compound("Level").and_then(|l| l.get_compound("Structures")).and_then(|s| s.get_compound("Starts")))
        } else {
            chunk_tag.get_compound("structures").and_then(|t| t.get_compound("starts"))
        };
        let starts = starts?;
        let known_starts = Self::load_structures(env, starts);
        let result = check_structure_info(&known_starts, structure, require_unreferenced);
        self.state().store_full_results(pos.pack(), known_starts);
        Some(result)
    }

    /// `loadStructures(chunkTag)` (`structures.starts` present).
    fn load_structures(env: &GenEnv, starts: &CompoundTag) -> HashMap<Identifier, i32> {
        let mut known = HashMap::new();
        for (key, tag) in starts.iter() {
            let Some(id) = Identifier::try_parse(key) else { continue };
            if env.structures.get(&id).is_none() {
                continue;
            }
            if let Some(data) = tag.as_compound()
                && data.get_string_or("id", "") != "INVALID"
            {
                known.insert(id, data.get_int_or("references", 0));
            }
        }
        known
    }

    /// `onStructureLoad(pos, starts)`: the valid starts of a chunk that just reached `STRUCTURE_STARTS`/was loaded
    /// (`structure id -> references`).
    pub fn on_structure_load(&self, pos: ChunkPos, valid_starts: HashMap<Identifier, i32>) {
        self.state().store_full_results(pos.pack(), valid_starts);
    }

    /// `onStructureLoad` from the chunk's raw `structures.starts` map.
    pub fn on_structure_load_raw(&self, pos: ChunkPos, data: &mc_world::chunk::StructureData) {
        let mut starts = HashMap::new();
        for (key, raw) in &data.starts {
            if let Some(id) = Identifier::try_parse(key)
                && raw.get_string_or("id", "") != "INVALID"
            {
                starts.insert(id, raw.get_int_or("references", 0));
            }
        }
        self.on_structure_load(pos, starts);
    }

    /// `incrementReference(chunkPos, structure)`.
    pub fn increment_reference(&self, chunk_pos: ChunkPos, structure: &Identifier) {
        let pos_key = chunk_pos.pack();
        let mut state = self.state();
        *state.loaded_chunks.entry(pos_key).or_default().entry(structure.clone()).or_insert(0) += 1;
        state.chunks_without_starts_in_storage.remove(&pos_key);
    }
}
