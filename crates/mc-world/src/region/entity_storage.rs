//! IO half of `net.minecraft.world.level.chunk.storage.EntityStorage` (`entities/` region files):
//! chunk tag layout `{DataVersion, Entities: [..], Position: [I; x, z]}` and the empty-chunk set.
//! Entity contents stay raw `CompoundTag`s; data fixing and entity instantiation are not part of it.

use super::simple_region_storage::SimpleRegionStorage;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, ListTag, Tag};
use std::collections::HashSet;
use std::io;

const ENTITIES_TAG: &str = "Entities";
const POSITION_TAG: &str = "Position";
/// `SharedConstants.getCurrentVersion().dataVersion().version()` (world version of 26.3).
pub const CURRENT_DATA_VERSION: i32 = 5023;

/// `ChunkEntities`
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkEntities<T = CompoundTag> {
    pos: ChunkPos,
    entities: Vec<T>,
}

impl<T> ChunkEntities<T> {
    pub fn new(pos: ChunkPos, entities: Vec<T>) -> Self {
        Self { pos, entities }
    }

    pub fn get_pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn get_entities(&self) -> &[T] {
        &self.entities
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

/// Why the stored `Position` of a loaded chunk was not usable (`loadEntities` reports these to the
/// server's chunk IO error reporter and keeps loading).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionProblem {
    /// "Chunk file at {} is in the wrong location."
    Misplaced { stored: ChunkPos },
    /// "Failed to parse chunk {} position info": `Position` absent or not a 2-int sequence.
    Unparseable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedEntities {
    pub chunk: ChunkEntities,
    pub position_problem: Option<PositionProblem>,
    /// Indices of `Entities` elements that are not compounds (vanilla reports and skips them).
    pub skipped_entries: Vec<usize>,
}

pub struct EntityStorage {
    storage: SimpleRegionStorage,
    empty_chunks: HashSet<i64>,
}

/// `ChunkPos.CODEC` decode (`Codec.INT_STREAM` + exactly two ints): int arrays and lists of numbers.
fn decode_chunk_pos(tag: &Tag) -> Option<ChunkPos> {
    let ints: Vec<i32> = match tag {
        Tag::IntArray(v) => v.clone(),
        Tag::ByteArray(v) => v.iter().map(|&b| b as i32).collect(),
        Tag::LongArray(v) => v.iter().map(|&l| l as i32).collect(),
        Tag::List(list) => list.iter().map(|t| t.as_number().map(|n| n.int_value())).collect::<Option<_>>()?,
        _ => return None,
    };
    match ints[..] {
        [x, z] => Some(ChunkPos::new(x, z)),
        _ => None,
    }
}

impl EntityStorage {
    pub fn new(storage: SimpleRegionStorage) -> Self {
        Self { storage, empty_chunks: HashSet::new() }
    }

    /// `loadEntities(pos)`
    pub fn load_entities(&mut self, pos: &ChunkPos) -> io::Result<LoadedEntities> {
        let empty = |problem| LoadedEntities { chunk: ChunkEntities::new(*pos, Vec::new()), position_problem: problem, skipped_entries: Vec::new() };
        if self.empty_chunks.contains(&pos.pack()) {
            return Ok(empty(None));
        }
        let Some(tag) = self.storage.read(pos)? else {
            self.empty_chunks.insert(pos.pack());
            return Ok(empty(None));
        };
        let position_problem = match tag.get(POSITION_TAG).and_then(decode_chunk_pos) {
            Some(stored) if stored == *pos => None,
            Some(stored) => {
                log::error!("Chunk file at {} is in the wrong location. (Expected {}, got {})", pos, pos, stored);
                Some(PositionProblem::Misplaced { stored })
            }
            None => {
                log::warn!("Failed to parse chunk {} position info", pos);
                Some(PositionProblem::Unparseable)
            }
        };
        let mut entities = Vec::new();
        let mut skipped_entries = Vec::new();
        if let Some(list) = tag.get_list(ENTITIES_TAG) {
            for (index, entry) in list.iter().enumerate() {
                match entry {
                    Tag::Compound(c) => entities.push(c.clone()),
                    _ => skipped_entries.push(index),
                }
            }
        }
        Ok(LoadedEntities { chunk: ChunkEntities::new(*pos, entities), position_problem, skipped_entries })
    }

    /// `storeEntities(chunk)`: empty chunks are written once (as removal); others as a full tag.
    pub fn store_entities(&mut self, chunk: &ChunkEntities) {
        let pos = chunk.get_pos();
        if chunk.is_empty() {
            if self.empty_chunks.insert(pos.pack()) {
                self.storage.write(&pos, None);
            }
        } else {
            let mut entities = ListTag::new();
            for e in chunk.get_entities() {
                entities.push(e.clone());
            }
            let mut chunk_tag = CompoundTag::new();
            chunk_tag.put_int("DataVersion", CURRENT_DATA_VERSION);
            chunk_tag.put(ENTITIES_TAG, entities);
            chunk_tag.put_int_array(POSITION_TAG, vec![pos.x, pos.z]);
            self.storage.write(&pos, Some(chunk_tag));
            self.empty_chunks.remove(&pos.pack());
        }
    }

    /// `flush(flushStorage)`
    pub fn flush(&mut self, flush_storage: bool) -> io::Result<()> {
        self.storage.synchronize(flush_storage)
    }

    pub fn close(&mut self) -> io::Result<()> {
        self.storage.close()
    }

    pub fn simple_region_storage(&mut self) -> &mut SimpleRegionStorage {
        &mut self.storage
    }
}
