//! `net.minecraft.world.level.chunk.storage.RecreatingSimpleRegionStorage`: reads from one region folder and writes every
//! chunk into a second one (the `new_<folder>` sibling of `--recreateRegionFiles`), which is deleted again on close.

use super::region_storage_info::RegionStorageInfo;
use super::simple_region_storage::SimpleRegionStorage;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::CompoundTag;
use std::io;
use std::path::PathBuf;

pub struct RecreatingSimpleRegionStorage {
    source: SimpleRegionStorage,
    target: SimpleRegionStorage,
    write_folder: PathBuf,
}

impl RecreatingSimpleRegionStorage {
    pub fn new(
        read_info: RegionStorageInfo,
        read_folder: impl Into<PathBuf>,
        write_info: RegionStorageInfo,
        write_folder: impl Into<PathBuf>,
        sync_writes: bool,
    ) -> Self {
        let write_folder = write_folder.into();
        Self {
            source: SimpleRegionStorage::new(read_info, read_folder, sync_writes),
            target: SimpleRegionStorage::new(write_info, write_folder.clone(), sync_writes),
            write_folder,
        }
    }

    /// `read(pos)`: from the source folder.
    pub fn read(&mut self, pos: &ChunkPos) -> io::Result<Option<CompoundTag>> {
        self.source.read(pos)
    }

    /// `write(pos, value)`: queued for the target folder.
    pub fn write(&mut self, pos: &ChunkPos, value: Option<CompoundTag>) {
        self.target.write(pos, value);
    }

    /// Waits for the queued writes (`join()` of the future `write` returned).
    pub fn join_writes(&mut self) -> io::Result<()> {
        self.target.synchronize(false)
    }

    /// `close()`: the source, then the target (a failed close is logged by the storage, like `IOWorker.close()`), then the
    /// target folder is deleted.
    pub fn close(&mut self) -> io::Result<()> {
        let _ = self.source.close();
        let _ = self.target.close();
        if self.write_folder.exists() {
            std::fs::remove_dir_all(&self.write_folder)?;
        }
        Ok(())
    }
}
