//! `ChunkLoadCounter`: counts how many chunks a scheduling action (tickets for the spawn area, ...) newly asks the chunk
//! source to bring to `FULL`, and how many of them have arrived (used by `prepareLevels`).

use super::ServerLevel;
use mc_world::status::ChunkStatus;
use std::collections::HashSet;

#[derive(Default)]
pub struct ChunkLoadCounter {
    pending_chunks: Vec<i64>,
    total_chunks: i32,
}

impl ChunkLoadCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// `track(level, scheduler)`: run `scheduler` and remember the chunks that became wanted at `FULL` because of it.
    pub fn track(&mut self, level: &mut ServerLevel, scheduler: impl FnOnce(&mut ServerLevel)) {
        level.run_distance_manager_updates();
        let already_loaded: HashSet<i64> = level
            .chunk_source
            .chunk_map
            .all_chunks_with_at_least_status(ChunkStatus::Full)
            .into_iter()
            .collect();
        scheduler(level);
        level.run_distance_manager_updates();
        for key in level
            .chunk_source
            .chunk_map
            .all_chunks_with_at_least_status(ChunkStatus::Full)
        {
            if !already_loaded.contains(&key) {
                self.pending_chunks.push(key);
                self.total_chunks += 1;
            }
        }
    }

    pub fn ready_chunks(&mut self, level: &ServerLevel) -> i32 {
        self.total_chunks - self.pending_chunks(level)
    }

    /// `pendingChunks()`: chunks not yet at latest status `FULL`.
    pub fn pending_chunks(&mut self, level: &ServerLevel) -> i32 {
        let map = &level.chunk_source.chunk_map;
        self.pending_chunks
            .retain(|key| map.get_latest_status(*key) != Some(ChunkStatus::Full));
        self.pending_chunks.len() as i32
    }

    pub fn total_chunks(&self) -> i32 {
        self.total_chunks
    }
}
