//! Synchronous `SimpleRegionStorage` / `IOWorker` semantics over [`RegionFileStorage`]: a pending
//! write map (insertion ordered, newest value wins, read-through), drained oldest-first on
//! [`synchronize`](SimpleRegionStorage::synchronize) and [`close`](SimpleRegionStorage::close).
//! There is no background thread: vanilla's one-chunk-per-idle-tick draining is replaced by an
//! explicit [`store_pending_chunk`](SimpleRegionStorage::store_pending_chunk).

use super::region_file_storage::RegionFileStorage;
use super::region_storage_info::RegionStorageInfo;
use indexmap::IndexMap;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::CompoundTag;
use std::io;
use std::path::PathBuf;

/// What runs when a queued chunk could not be written (`IOWorker.runStore` completes the write exceptionally).
pub type StoreFailureHandler = Box<dyn Fn(ChunkPos, &io::Error) + Send>;

pub struct SimpleRegionStorage {
    storage: RegionFileStorage,
    /// `IOWorker.pendingWrites`; `None` = clear the chunk (`IOWorker.STORE_EMPTY`).
    pending_writes: IndexMap<i64, Option<CompoundTag>>,
    on_store_failure: Option<StoreFailureHandler>,
}

impl SimpleRegionStorage {
    pub fn new(info: RegionStorageInfo, folder: impl Into<PathBuf>, sync_writes: bool) -> Self {
        Self {
            storage: RegionFileStorage::new(info, folder, sync_writes),
            pending_writes: IndexMap::new(),
            on_store_failure: None,
        }
    }

    /// Sets what runs for a chunk whose write failed (after the failure is logged).
    pub fn set_store_failure_handler(&mut self, handler: StoreFailureHandler) {
        self.on_store_failure = Some(handler);
    }

    /// `read(pos)`: pending write first (a copy), else the stored chunk.
    pub fn read(&mut self, pos: &ChunkPos) -> io::Result<Option<CompoundTag>> {
        if let Some(pending) = self.pending_writes.get(&pos.pack()) {
            return Ok(pending.clone());
        }
        self.storage
            .read(pos)
            .inspect_err(|e| log::warn!("Failed to read chunk {}: {}", pos, e))
    }

    /// `write(pos, value)`: queues the write (`None` = remove the chunk). An already queued chunk
    /// keeps its queue position and takes the new value.
    pub fn write(&mut self, pos: &ChunkPos, value: Option<CompoundTag>) {
        self.pending_writes.insert(pos.pack(), value);
    }

    /// `storePendingChunk`: persists the oldest queued chunk; returns whether there was one.
    pub fn store_pending_chunk(&mut self) -> io::Result<bool> {
        let Some((key, data)) = self.pending_writes.shift_remove_index(0) else {
            return Ok(false);
        };
        let pos = ChunkPos::unpack(key);
        self.storage.write(&pos, data.as_ref()).inspect_err(|e| {
            log::error!("Failed to store chunk {}: {}", pos, e);
            if let Some(handler) = &self.on_store_failure {
                handler(pos, e);
            }
        })?;
        Ok(true)
    }

    /// `scanChunk(pos, visitor)`: the chunk's stored tag without creating a region file, a queued write first (`None`
    /// where it removes the chunk).
    pub fn scan_chunk(&mut self, pos: &ChunkPos) -> io::Result<Option<CompoundTag>> {
        if let Some(pending) = self.pending_writes.get(&pos.pack()) {
            return Ok(pending.clone());
        }
        self.storage
            .read(pos)
            .inspect_err(|e| log::warn!("Failed to bulk scan chunk {}: {}", pos, e))
    }

    /// `synchronize(flush)`: persists every queued chunk (continuing past failures, returning the
    /// first), then optionally forces the region files to disk.
    pub fn synchronize(&mut self, flush: bool) -> io::Result<()> {
        let mut first_error = None;
        while !self.pending_writes.is_empty() {
            if let Err(e) = self.store_pending_chunk() {
                first_error.get_or_insert(e);
            }
        }
        if let Some(e) = first_error {
            return Err(e);
        }
        if flush {
            self.storage
                .flush()
                .inspect_err(|e| log::warn!("Failed to synchronize chunks: {}", e))?;
        }
        Ok(())
    }

    /// `close()`: drains queued writes, then closes the region files.
    pub fn close(&mut self) -> io::Result<()> {
        while !self.pending_writes.is_empty() {
            let _ = self.store_pending_chunk();
        }
        self.storage
            .close()
            .inspect_err(|e| log::error!("Failed to close storage: {}", e))
    }

    pub fn storage_info(&self) -> &RegionStorageInfo {
        self.storage.info()
    }

    pub fn region_file_storage(&mut self) -> &mut RegionFileStorage {
        &mut self.storage
    }
}
