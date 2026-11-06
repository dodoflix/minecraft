//! `net.minecraft.world.level.chunk.storage.RegionFileStorage`: LRU cache of open region files of one
//! folder (`region/`, `entities/`, `poi/`), reading and writing whole chunk NBT.

use super::region_file::RegionFile;
use super::region_storage_info::RegionStorageInfo;
use indexmap::IndexMap;
use mc_core::pos::chunk_pos::ChunkPos;
use mc_nbt::{CompoundTag, NbtAccounter, NbtError};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

pub const ANVIL_EXTENSION: &str = ".mca";
const MAX_CACHE_SIZE: usize = 256;

/// NBT failures reach callers as `IOException`/runtime exceptions in vanilla; here all are `io::Error`.
pub fn nbt_io_error(e: NbtError) -> io::Error {
    match e {
        NbtError::Eof => io::Error::from(io::ErrorKind::UnexpectedEof),
        other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
    }
}

pub struct RegionFileStorage {
    /// Most recently used entry last (`Long2ObjectLinkedOpenHashMap` with `getAndMoveToFirst`);
    /// `None` caches "no such region file".
    region_cache: IndexMap<i64, Option<RegionFile>>,
    info: RegionStorageInfo,
    folder: PathBuf,
    sync: bool,
}

impl RegionFileStorage {
    pub fn new(info: RegionStorageInfo, folder: impl Into<PathBuf>, sync: bool) -> Self {
        Self {
            region_cache: IndexMap::new(),
            info,
            folder: folder.into(),
            sync,
        }
    }

    fn region_path(&self, pos: &ChunkPos) -> PathBuf {
        self.folder.join(format!(
            "r.{}.{}{}",
            pos.get_region_x(),
            pos.get_region_z(),
            ANVIL_EXTENSION
        ))
    }

    /// `getRegionFile(pos, create)`
    pub fn get_region_file(
        &mut self,
        pos: &ChunkPos,
        create: bool,
    ) -> io::Result<Option<&mut RegionFile>> {
        let key = ChunkPos::pack_xz(pos.get_region_x(), pos.get_region_z());
        if let Some(index) = self.region_cache.get_index_of(&key) {
            let last = self.region_cache.len() - 1;
            self.region_cache.move_index(index, last);
            if self.region_cache[last].is_some() || !create {
                return Ok(self.region_cache[last].as_mut());
            }
        }
        let path = self.region_path(pos);
        if !create && !path.is_file() {
            self.cache(key, None)?;
            return Ok(None);
        }
        fs::create_dir_all(&self.folder)?;
        let new_region = RegionFile::new(self.info.clone(), &path, &self.folder, self.sync)?;
        self.cache(key, Some(new_region))?;
        Ok(self.region_cache.last_mut().and_then(|(_, r)| r.as_mut()))
    }

    fn cache(&mut self, key: i64, entry: Option<RegionFile>) -> io::Result<()> {
        // putAndMoveToFirst: replace and mark most recently used.
        self.region_cache.shift_remove(&key);
        self.region_cache.insert(key, entry);
        if self.region_cache.len() > MAX_CACHE_SIZE {
            if let Some((_, Some(evicted))) = self.region_cache.shift_remove_index(0) {
                evicted.close()?;
            }
        }
        Ok(())
    }

    /// `read(pos)`: the chunk's NBT (`NbtIo.read` with an unlimited-heap accounter), `None` where
    /// vanilla returns null.
    pub fn read(&mut self, pos: &ChunkPos) -> io::Result<Option<CompoundTag>> {
        let Some(region) = self.get_region_file(pos, false)? else {
            return Ok(None);
        };
        let Some(mut stream) = region.get_chunk_data_input_stream(pos)? else {
            return Ok(None);
        };
        // Vanilla parses incrementally and never looks past the end of the root compound, so a
        // decoding error after a complete tag is irrelevant: parse whatever was decoded first.
        let mut data = Vec::new();
        let read_error = stream.read_to_end(&mut data).err();
        match mc_nbt::read_named(&mut data.as_slice(), &mut NbtAccounter::unlimited_heap()) {
            Ok(tag) => Ok(Some(tag)),
            Err(e) => Err(read_error.unwrap_or_else(|| nbt_io_error(e))),
        }
    }

    /// `write(pos, value)`: `None` clears the chunk.
    pub fn write(&mut self, pos: &ChunkPos, value: Option<&CompoundTag>) -> io::Result<()> {
        let region = self
            .get_region_file(pos, true)?
            .expect("create=true always yields a region file");
        match value {
            None => region.clear(pos),
            Some(tag) => region.write_chunk_data(pos, &mc_nbt::nbt_io::to_bytes(tag)),
        }
    }

    /// `close()`: closes every cached region file, reporting the first failure after closing all.
    pub fn close(&mut self) -> io::Result<()> {
        let mut first_error = None;
        for (_, entry) in self.region_cache.drain(..).rev() {
            if let Some(region) = entry {
                if let Err(e) = region.close() {
                    first_error.get_or_insert(e);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub fn flush(&self) -> io::Result<()> {
        for region in self.region_cache.values().rev().flatten() {
            region.flush()?;
        }
        Ok(())
    }

    pub fn info(&self) -> &RegionStorageInfo {
        &self.info
    }

    /// Open (cached) region files, most recently used first (test/diagnostic helper).
    pub fn cached_region_keys(&self) -> Vec<i64> {
        self.region_cache.keys().rev().copied().collect()
    }
}
