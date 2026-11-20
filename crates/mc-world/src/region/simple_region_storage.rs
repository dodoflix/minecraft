//! Synchronous `SimpleRegionStorage` / `IOWorker` semantics over [`RegionFileStorage`]: a pending
//! write map (insertion ordered, newest value wins, read-through), drained oldest-first on
//! [`synchronize`](SimpleRegionStorage::synchronize) and [`close`](SimpleRegionStorage::close).
//! There is no background thread: vanilla's one-chunk-per-idle-tick draining is replaced by an
//! explicit [`store_pending_chunk`](SimpleRegionStorage::store_pending_chunk).

use super::old_chunks::{OldChunkBits, OldChunkRegions, is_old_chunk};
use super::region_file_storage::RegionFileStorage;
use super::region_storage_info::RegionStorageInfo;
use indexmap::IndexMap;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::stream::{CollectFields, FieldSelector, StreamTagVisitor};
use mc_nbt::{CompoundTag, TAG_COMPOUND, TAG_INT, Tag};
use std::io;
use std::path::PathBuf;

/// What runs when a queued chunk could not be written (`IOWorker.runStore` completes the write exceptionally).
pub type StoreFailureHandler = Box<dyn Fn(ChunkPos, &io::Error) + Send>;

pub struct SimpleRegionStorage {
    storage: RegionFileStorage,
    /// `IOWorker.pendingWrites`; `None` = clear the chunk (`IOWorker.STORE_EMPTY`).
    pending_writes: IndexMap<i64, Option<CompoundTag>>,
    on_store_failure: Option<StoreFailureHandler>,
    /// `IOWorker.regionCacheForBlender`.
    old_regions: OldChunkRegions,
}

impl SimpleRegionStorage {
    pub fn new(info: RegionStorageInfo, folder: impl Into<PathBuf>, sync_writes: bool) -> Self {
        Self {
            storage: RegionFileStorage::new(info, folder, sync_writes),
            pending_writes: IndexMap::new(),
            on_store_failure: None,
            old_regions: OldChunkRegions::default(),
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

    /// `scanChunk(pos, visitor)`: the stored chunk streamed through `visitor`, a queued write first (nothing for one that
    /// removes the chunk).
    pub fn scan_chunk_with(
        &mut self,
        pos: &ChunkPos,
        visitor: &mut dyn StreamTagVisitor,
    ) -> io::Result<()> {
        match self.pending_writes.get(&pos.pack()) {
            Some(Some(tag)) => {
                Tag::Compound(tag.clone()).accept_as_root(visitor);
                Ok(())
            }
            Some(None) => Ok(()),
            None => self
                .storage
                .scan_chunk(pos, visitor)
                .inspect_err(|e| log::warn!("Failed to bulk scan chunk {}: {}", pos, e)),
        }
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

    /// `isOldChunkAround(pos, range)`: whether a chunk within `range` chunks of `pos` was stored before `DataVersion` 4997
    /// or with `blending_data`. A region is scanned once and its answer stays until 1 024 other regions were asked for
    /// (chunks saved meanwhile do not change it).
    pub fn is_old_chunk_around(&mut self, pos: ChunkPos, range: i32) -> bool {
        let regions = self.old_regions.clone();
        regions.is_old_chunk_around(pos, range, |region_x, region_z| {
            self.scan_old_region(region_x, region_z)
        })
    }

    /// The cache `isOldChunkAround` answers from, for a caller that asks without going through the storage (the scan of an
    /// uncached region is [`Self::scan_old_region`]).
    pub fn old_chunk_regions(&self) -> OldChunkRegions {
        self.old_regions.clone()
    }

    /// `IOWorker.createOldDataForRegion`: the bits of one region file.
    pub fn scan_old_region(&mut self, region_x: i32, region_z: i32) -> OldChunkBits {
        let mut bits = OldChunkBits::default();
        let min = ChunkPos::min_from_region(region_x, region_z);
        for local_z in 0..ChunkPos::REGION_SIZE {
            for local_x in 0..ChunkPos::REGION_SIZE {
                let pos = ChunkPos::new(min.x + local_x, min.z + local_z);
                let mut collect = CollectFields::new(&[
                    FieldSelector::new(TAG_INT, "DataVersion"),
                    FieldSelector::new(TAG_COMPOUND, "blending_data"),
                ]);
                match self.scan_chunk_with(&pos, &mut collect) {
                    Ok(()) => {
                        if let Some(Tag::Compound(tag)) = collect.get_result()
                            && is_old_chunk(tag)
                        {
                            bits.set(local_x, local_z);
                        }
                    }
                    Err(e) => log::warn!("Failed to scan chunk {}: {}", pos, e),
                }
            }
        }
        bits
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
